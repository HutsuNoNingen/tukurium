use std::io::{self, BufRead};
use std::process;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::events::{Event, EventBody, EventBusHandle};

pub struct Supervisor {
    processes: Vec<Mutex<process::Child>>,
    bus_writer: mpsc::Sender<Event>,
    bus_reader: mpsc::Receiver<Event>,
}

impl Supervisor {
    pub fn new() -> Supervisor {
        let (bus_writer, bus_reader) = mpsc::channel::<Event>();
        Supervisor {
            processes: vec![],
            bus_writer: bus_writer,
            bus_reader: bus_reader,
        }
    }

    pub fn spawn(&mut self, task_name: &str, program: &str, args: Vec<&str>) {
        // Take ownership
        let bus_writer = self.bus_writer.clone();
        let task_name = task_name.to_string();
        let program = program.to_string();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();

        // Create event bus handle
        let bus = EventBusHandle {
            sender: Arc::new(bus_writer),
            task_name: task_name,
        };

        // Start thread!
        bus.log(format!("run {} args {:?}", program, args));

        let mut cmd = match process::Command::new(program)
            .args(args)
            .stdin(process::Stdio::null())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()
        {
            Ok(v) => v,
            Err(err) => {
                bus.fatal_error(format!("Could not spawn process: {:?}", err));
                return;
            }
        };

        bus.log(format!("spawned {}", cmd.id()));

        // stdout log
        {
            let bus = bus.clone();
            let mut stdout = cmd.stdout.take();
            thread::spawn(move || {
                let lines = io::BufReader::new(
                    stdout.as_mut().expect("Supervised task MUST have a stdout"),
                )
                .lines();
                for line in lines.map_while(Result::ok) {
                    bus.log(format!("out: {}", line));
                }

                bus.log(format!("stdout closed, time to reap this process!"));
                bus.announce_dead();
                drop(stdout);
            });
        }

        // stderr log
        {
            let bus = bus.clone();
            let mut stderr = cmd.stderr.take();
            thread::spawn(move || {
                let lines = io::BufReader::new(
                    stderr.as_mut().expect("Supervised task MUST have a stderr"),
                )
                .lines();
                for line in lines.map_while(Result::ok) {
                    bus.log(format!("out: {}", line));
                }
                bus.log(format!("stderr closed"));
                drop(stderr);
            });
        }

        // Add process to list
        self.processes.push(Mutex::new(cmd));
    }

    pub fn run_bus_reader_until_dead(&mut self) {
        while let Ok(event) = self.bus_reader.recv() {
            match event.body {
                EventBody::Log(s) => println!("msg: {}: {}", event.sender, s),
                EventBody::Dead => {
                    println!("dead: {}", event.sender);
                    return;
                }
            }
        }
    }

    pub fn reap_processes(&mut self) {
        // Kill everything
        for cmd in self.processes.drain(0..) {
            let mut cmd = cmd
                .lock()
                .expect("Could not lock process to kill and reap it");
            let pid = cmd.id();
            println!("ID = {}", pid);

            // On Unix-likes, Child.kill() does SIGKILL.
            // This means that it can't clean anything up.
            // So here, we do SIGTERM instead.
            #[cfg(unix)]
            {
                unsafe {
                    _ = libc::kill(
                        pid.try_into().expect("Could not kill process"),
                        libc::SIGTERM,
                    );
                }
            }
            // On Windows... is there a nice way to clean this up?
            // Child.kill() probably does TerminateProcess().
            #[cfg(not(unix))]
            {
                cmd.kill().expect("Could not kill process");
            }

            match cmd.wait() {
                Ok(status) => {
                    println!("process {} exit status = {:?}", pid, status);
                }
                Err(err) => {
                    eprintln!("ERROR: Could not reap process: {:?}", err);
                }
            }
        }

        // All *direct* processes are now dead.
        // But some processes spawn child processes in an annoying way.
        // So we can only drain until there's nothing left to read.
        // The indirect processes can be reaped when this supervisor shuts down.

        // Drain bus
        println!("Draining the event bus until it's empty (no blocking)...");
        while let Ok(event) = self.bus_reader.try_recv() {
            match event.body {
                EventBody::Log(s) => println!("msg: {}: {}", event.sender, s),
                EventBody::Dead => {
                    println!("dead: {}", event.sender);
                }
            }
        }
    }
}
