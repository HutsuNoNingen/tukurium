mod config;
mod events;
mod supervisor;

use std::env;

use crate::config::start_processes_from_config;
use crate::supervisor::Supervisor;

// TODO: Get -ServerAddress default from distribution/connection.json --GM
const DEFAULT_SERVER: &str = "45.77.130.146";

fn main() {
    println!("やっほー");

    let mut supervisor = Supervisor::new();

    let mut online_server: Option<String> = None;
    {
        let mut args = env::args();
        _ = args.next(); // Skip executable path when parsing arguments
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-ServerAddress" => {
                    online_server = Some(
                        args.next()
                            .expect("ERROR: expected address after -ServerAddress"),
                    );
                }
                "-Online" => {
                    if online_server == None {
                        online_server = Some(DEFAULT_SERVER.to_string());
                    }
                }
                _ => {
                    eprintln!("ERROR: invalid argument: \"{}\"", arg);
                }
            }
        }
    }
    start_processes_from_config(&mut supervisor, online_server);

    // Go!
    supervisor.run_bus_reader_until_dead();

    // Clean up
    supervisor.reap_processes();
    println!("おつかれ");
}
