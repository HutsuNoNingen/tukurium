use std::io::{self, BufRead};
use std::process;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use crate::events::{Event, EventBody, EventBusHandle};

pub struct Process {
    child: Box<process::Child>,
    name: String,
}
pub struct Supervisor {
    processes: Vec<Process>,
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

    pub fn spawn(&mut self, process_name: &str, program: &str, args: Vec<&str>) {
        // Take ownership
        let program = program.to_string();
        let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();

        // Create event bus handle
        let bus = EventBusHandle {
            sender: Arc::new(self.bus_writer.clone()),
            process_name: process_name.to_string(),
        };

        // Start thread!
        bus.log(format!("run {} args {:?}", program, args));

        let mut cmd = match process::Command::new(if cfg!(windows) {
            format!("{}.exe", program)
        } else {
            program
        })
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
        self.processes.push(Process {
            child: Box::new(cmd),
            name: process_name.to_string(),
        });
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

    pub fn reap_processes(self) {
        // Remove the cloneable bus_writer
        // Otherwise this part will hang, waiting for a bus_writer that will never be read
        let mut processes = self.processes;
        let bus_reader = self.bus_reader;
        drop(self.bus_writer);

        // Kill everything
        for proc in processes.drain(0..) {
            let mut cmd = *proc.child;
            let pid = cmd.id();
            println!("process {} ID = {}", proc.name, pid);

            if cfg!(unix) {
                // On Unix-likes, Child.kill() does SIGKILL.
                // This means that it can't clean anything up.
                // So here, we do SIGINT instead.
                //
                // Normally you're supposed to use SIGTERM,
                // but tokio requires OS-specific things to handle stuff that isn't SIGINT.
                unsafe {
                    _ = libc::kill(
                        pid.try_into().expect("Could not kill process"),
                        libc::SIGINT,
                    );
                }
            } else {
                // On Windows... is there a nice way to clean this up?
                // Child.kill() probably does TerminateProcess().
                cmd.kill().expect("Could not kill process");
            }

            match cmd.wait() {
                Ok(status) => {
                    println!("process {} ({}) exit status = {:?}", proc.name, pid, status);
                }
                Err(err) => {
                    eprintln!("ERROR: Could not reap process {}: {:?}", proc.name, err);
                }
            }
        }

        // All *direct* processes are now dead.
        // But some processes spawn child processes in an annoying way.
        // Specifically, via tokio::process::Command().
        //
        // stdout and stderr are inherited from our direct child.
        // For some reason, if we do SIGTERM or SIGKILL, the indirect child's stdout/stderr are not closed.
        // The reason SIGINT works is because tokio can handle it properly.
        //
        // TODO: Confirm Windows behaviour.

        // Drain bus
        println!("Draining the event bus until it's empty..");
        while let Ok(event) = bus_reader.recv() {
            match event.body {
                EventBody::Log(s) => println!("msg: {}: {}", event.sender, s),
                EventBody::Dead => {
                    println!("dead: {}", event.sender);
                }
            }
        }
    }
}
