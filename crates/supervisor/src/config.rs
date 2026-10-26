use crate::supervise::{Supervisor};

pub fn start_processes_from_config(supervisor: &mut Supervisor) {
    let session_name = "demo2";
    let profile_name = "default";

    supervisor.spawn(
        "relay",
        "./.tools/kokosync-relay",
        vec!["--http", "127.0.0.1:48000", "--quic", "127.0.0.1:48001"],
    );

    supervisor.spawn(
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

    supervisor.spawn(
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

    supervisor.spawn(
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
