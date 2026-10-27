//! Console output and Windows console-close handling. Save on the async main
//! thread; the OS callback only requests shutdown and waits for it to finish.
use std::{
    fmt,
    fs::{File, OpenOptions},
    io::Write,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
static CLOSING: AtomicBool = AtomicBool::new(false);
static CLOSED: AtomicBool = AtomicBool::new(false);

pub fn log(args: fmt::Arguments<'_>) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!(
        "[{:02}:{:02}:{:02} UTC] {args}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    );
    std::eprintln!("{line}");
    if let Some(file) = LOG.get()
        && let Ok(mut file) = file.lock()
    {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

pub struct ShutdownGuard;
impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        CLOSED.store(true, Ordering::Release);
    }
}

pub fn initialize(args: &[String]) -> Result<ShutdownGuard, std::io::Error> {
    if let Some(pair) = args.windows(2).find(|w| w[0] == "--log-file") {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&pair[1])?;
        let _ = LOG.set(Mutex::new(file));
    }
    #[cfg(windows)]
    if args.iter().any(|arg| arg == "--console") {
        let title = args
            .windows(2)
            .find(|w| w[0] == "--console-title")
            .map_or("tukurium Engine", |w| w[1].as_str());
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        // SAFETY: static callback and a NUL-terminated title live for each call.
        unsafe {
            SetConsoleTitleW(title.as_ptr());
            if SetConsoleCtrlHandler(Some(console_event), 1) == 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        log(format_args!(
            "Close this console (X) or press Ctrl+C to save and stop."
        ));
    }
    Ok(ShutdownGuard)
}

pub async fn close_requested() {
    while !CLOSING.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn SetConsoleTitleW(title: *const u16) -> i32;
}

#[cfg(windows)]
unsafe extern "system" fn console_event(event: u32) -> i32 {
    // CTRL_C, CTRL_BREAK, CTRL_CLOSE, CTRL_LOGOFF, CTRL_SHUTDOWN.
    if !matches!(event, 0 | 1 | 2 | 5 | 6) {
        return 0;
    }
    CLOSING.store(true, Ordering::Release);
    let deadline = std::time::Instant::now() + Duration::from_millis(4500);
    while !CLOSED.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    1
}
