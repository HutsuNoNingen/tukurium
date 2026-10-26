mod config;
mod events;
mod supervise;

use crate::config::{start_processes_from_config};
use crate::supervise::{Supervisor};

fn main() {
    println!("やっほー");

    let mut supervisor = Supervisor::new();

    start_processes_from_config(&mut supervisor);

    // Go!
    supervisor.run_bus_reader_until_dead();

    // Clean up
    supervisor.reap_processes();
    println!("おつかれ");
}
