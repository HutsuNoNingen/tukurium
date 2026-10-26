//! Cooperative execution deadlines. Windows uses current-thread CPU time so
//! scheduling delays do not consume the CPU target, plus a wall-time watchdog.
//! GetThreadTimes reports 100ns units, not a guaranteed measurement resolution;
//! consequently the CPU target is sampled and soft, rather than a hard deadline.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const CPU_TARGET_TICKS: u64 = 50_000;
const WALL_FALLBACK: Duration = Duration::from_millis(5);
const WALL_WATCHDOG: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LimitKind {
    CpuTime,
    WallTime,
    WallWatchdog,
}

impl LimitKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::CpuTime => "CPU time limit",
            Self::WallTime => "time limit",
            Self::WallWatchdog => "wall watchdog",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CpuSample {
    thread_id: u32,
    ticks: u64,
}

fn cpu_elapsed(start: Option<CpuSample>, current: Option<CpuSample>) -> Option<u64> {
    let (start, current) = (start?, current?);
    if start.thread_id != current.thread_id {
        return None;
    }
    current.ticks.checked_sub(start.ticks)
}

fn evaluate(
    wall_elapsed: Duration,
    start: Option<CpuSample>,
    current: Option<CpuSample>,
) -> Option<LimitKind> {
    if wall_elapsed >= WALL_WATCHDOG {
        return Some(LimitKind::WallWatchdog);
    }
    match cpu_elapsed(start, current) {
        Some(ticks) => (ticks >= CPU_TARGET_TICKS).then_some(LimitKind::CpuTime),
        None => (wall_elapsed >= WALL_FALLBACK).then_some(LimitKind::WallTime),
    }
}

pub(super) struct Deadline {
    started: Instant,
    cpu_start: Option<CpuSample>,
    wall_fallback: AtomicBool,
}

impl Deadline {
    pub(super) fn start() -> Self {
        let started = Instant::now();
        let cpu_start = current_cpu_sample();
        Self {
            started,
            cpu_start,
            wall_fallback: AtomicBool::new(cpu_start.is_none()),
        }
    }

    pub(super) fn exceeded(&self) -> Option<LimitKind> {
        let current = if self.wall_fallback.load(Ordering::Relaxed) {
            None
        } else {
            current_cpu_sample()
        };
        self.evaluate_sample(self.started.elapsed(), current)
    }

    fn evaluate_sample(
        &self,
        wall_elapsed: Duration,
        current: Option<CpuSample>,
    ) -> Option<LimitKind> {
        let start = if self.wall_fallback.load(Ordering::Relaxed) {
            None
        } else {
            self.cpu_start
        };
        if start.is_some() && cpu_elapsed(start, current).is_none() {
            // Once the CPU clock becomes unreliable, retain the original wall
            // start for this entire phase. A later successful query grants no
            // fresh budget. Deadline::start() resets this for the next phase.
            self.wall_fallback.store(true, Ordering::Relaxed);
        }
        evaluate(wall_elapsed, start, current)
    }
}

#[cfg(not(windows))]
fn current_cpu_sample() -> Option<CpuSample> {
    None
}

#[cfg(windows)]
fn current_cpu_sample() -> Option<CpuSample> {
    windows::current_cpu_sample()
}

#[cfg(windows)]
mod windows {
    use super::CpuSample;
    use std::ffi::c_void;

    // FILETIME is two DWORDs, low word first. Durations use 100ns units.
    // https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetCurrentThread"]
        fn get_current_thread() -> *mut c_void;
        #[link_name = "GetCurrentThreadId"]
        fn get_current_thread_id() -> u32;
        #[link_name = "GetThreadTimes"]
        fn get_thread_times(
            thread: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    pub(super) fn current_cpu_sample() -> Option<CpuSample> {
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        // SAFETY: These documented Kernel32 functions operate on the calling
        // thread. Its pseudo handle needs no OpenThread or CloseHandle. Every
        // output pointer addresses a distinct initialized FILETIME with the
        // documented C layout and remains valid for this synchronous call.
        // BOOL is a 32-bit signed integer, DWORD is u32, HANDLE is a pointer.
        // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes
        // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getcurrentthread
        let (thread_id, success) = unsafe {
            (
                get_current_thread_id(),
                get_thread_times(
                    get_current_thread(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                ),
            )
        };
        if success == 0 || thread_id == 0 {
            return None;
        }
        Some(CpuSample {
            thread_id,
            ticks: kernel.ticks().checked_add(user.ticks())?,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn filetime_layout_and_word_order_match_windows() {
            assert_eq!(std::mem::size_of::<FileTime>(), 8);
            assert_eq!(std::mem::align_of::<FileTime>(), 4);
            assert_eq!(FileTime { low: 7, high: 2 }.ticks(), (2_u64 << 32) | 7);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(thread_id: u32, ticks: u64) -> Option<CpuSample> {
        Some(CpuSample { thread_id, ticks })
    }

    fn deadline(start: Option<CpuSample>) -> Deadline {
        Deadline {
            started: Instant::now(),
            cpu_start: start,
            wall_fallback: AtomicBool::new(start.is_none()),
        }
    }

    #[test]
    fn descheduling_does_not_consume_the_cpu_target() {
        assert_eq!(
            evaluate(Duration::from_millis(50), sample(1, 100), sample(1, 10_100)),
            None
        );
        assert_eq!(
            evaluate(Duration::from_millis(99), sample(1, 100), sample(1, 50_099)),
            None
        );
        assert_eq!(
            evaluate(Duration::from_millis(8), sample(1, 100), sample(1, 50_100)),
            Some(LimitKind::CpuTime)
        );
    }

    #[test]
    fn wall_watchdog_bounds_even_a_valid_idle_cpu_clock() {
        assert_eq!(
            evaluate(Duration::from_millis(100), sample(1, 100), sample(1, 100)),
            Some(LimitKind::WallWatchdog)
        );
        assert_eq!(
            evaluate(Duration::from_millis(101), None, None),
            Some(LimitKind::WallWatchdog)
        );
    }

    #[test]
    fn unreliable_cpu_samples_use_the_original_wall_limit() {
        for (start, current) in [
            (None, None),
            (None, sample(1, 100)),
            (sample(1, 100), None),
            (sample(1, 100), sample(1, 99)),
            (sample(1, 100), sample(2, 101)),
        ] {
            assert_eq!(evaluate(Duration::from_millis(4), start, current), None);
            assert_eq!(
                evaluate(Duration::from_millis(5), start, current),
                Some(LimitKind::WallTime)
            );
            assert_eq!(
                evaluate(Duration::from_millis(50), start, current),
                Some(LimitKind::WallTime)
            );
        }
    }

    #[test]
    fn fallback_is_sticky_but_a_new_phase_has_a_fresh_budget() {
        let old = deadline(sample(1, 100));
        assert_eq!(old.evaluate_sample(Duration::from_millis(2), None), None);
        assert_eq!(
            old.evaluate_sample(Duration::from_millis(6), sample(1, 101)),
            Some(LimitKind::WallTime)
        );
        let new = deadline(sample(1, 101));
        assert_eq!(new.evaluate_sample(Duration::ZERO, sample(1, 101)), None);
        assert_eq!(
            new.evaluate_sample(Duration::from_millis(50), sample(1, 10_101)),
            None
        );
    }

    #[test]
    fn labels_identify_the_budget_that_expired() {
        assert_eq!(LimitKind::CpuTime.label(), "CPU time limit");
        assert_eq!(LimitKind::WallTime.label(), "time limit");
        assert_eq!(LimitKind::WallWatchdog.label(), "wall watchdog");
    }

    #[cfg(windows)]
    #[test]
    fn native_samples_are_monotonic_on_the_calling_thread() {
        // Availability is not assumed: failure is supported by the wall fallback.
        // There is deliberately no assertion about sub-millisecond resolution.
        if let Some(first) = current_cpu_sample()
            && let Some(second) = current_cpu_sample()
        {
            assert_ne!(first.thread_id, 0);
            assert_eq!(first.thread_id, second.thread_id);
            assert!(second.ticks >= first.ticks);
        }
        // Exercise the real entry points without a timing-sensitive assertion.
        let _ = Deadline::start().exceeded();
    }
}
