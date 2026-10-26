mod config;
mod events;
mod supervise;

use std::process;
use std::sync::mpsc;
use std::sync::{Mutex};

use crate::config::{start_processes_from_config};
use crate::events::{Event};
use crate::events::{run_bus_reader_until_dead};
use crate::supervise::{reap_processes};

fn main() {
    println!("やっほー");

    // Collect all logs here.
    let (bus_writer, bus_reader) = mpsc::channel::<Event>();

    let mut processes = Vec::<Mutex<process::Child>>::new();

    start_processes_from_config(&mut processes, &bus_writer);
    println!("processes: {}", processes.len());

    // Go!
    drop(bus_writer);
    run_bus_reader_until_dead(&bus_reader);

    reap_processes(processes, &bus_reader);
    println!("おつかれ");
}
