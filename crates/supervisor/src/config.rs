use std::process;
use std::sync::{Mutex};
use std::sync::mpsc;

use crate::events::{Event};
use crate::supervise::supervise;

pub fn start_processes_from_config(processes: &mut Vec<Mutex<process::Child>>, bus_writer: &mpsc::Sender<Event>) {
    let session_name = "demo2";
    let profile_name = "default";

    supervise(
        processes,
        bus_writer,
        "relay",
        "./.tools/kokosync-relay",
        vec!["--http", "127.0.0.1:48000", "--quic", "127.0.0.1:48001"],
    );

    supervise(
        processes,
        bus_writer,
        "asset-server",
        "./target/debug/kokoverse-asset-server",
        vec![
            "--listen",
            "127.0.0.1:8787",
            "--data-dir",
            ".run/local/server",
            "--relay",
            "127.0.0.1:48001",
        ],
    );

    supervise(
        processes,
        bus_writer,
        "engine",
        "./target/debug/kokoverse-engine",
        vec![
            "--server",
            "http://127.0.0.1:8787",
            "--port",
            "47000",
            "--transport-port",
            "47001",
            "--profile",
            &format!(".run/Kokoverse/profiles/{}/profile.dat", profile_name).to_string(),
            "--data-dir",
            &format!(".run/local/{}", profile_name).to_string(),
            "--asset-dir",
            &format!(".run/local/{}/assets", profile_name).to_string(),
            "--adapter",
            &format!("{}", "./.tools/kokosync-adapter").to_string(),
            "--session",
            &format!("{}", session_name).to_string(),
        ],
    );

    supervise(
        processes,
        bus_writer,
        "gui",
        "godot",
        vec![
            "--path",
            "client-godot",
            "--",
            "--port=47000",
            &format!("--session={}", session_name).to_string(),
            //"--close-with-engine",
        ],
    );
}
