use crate::supervisor::Supervisor;

use sha2::Digest;

pub fn start_processes_from_config(supervisor: &mut Supervisor, online_address: Option<String>) {
    let profile_name = "default";
    let session_name = "lobby";

    supervisor.spawn(
        "relay",
        ".tools/kokosync-relay",
        vec!["--http", "127.0.0.1:48000", "--quic", "127.0.0.1:48001"],
    );

    match online_address {
        // Local
        None => {
            supervisor.spawn(
                "asset-server",
                "target/debug/kokoverse-asset-server",
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
                "target/debug/kokoverse-engine",
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
                    if cfg!(windows) {
                        "./.tools/kokosync-adapter.exe"
                    } else {
                        "./.tools/kokosync-adapter"
                    },
                    "--session",
                    &format!("{}", session_name).to_string(),
                ],
            );
        },

        // Online
        Some(server_address) => {
            let server_name_hash = format!("{:x}", sha2::Sha256::digest(&server_address));

            supervisor.spawn(
                "engine",
                "target/debug/kokoverse-engine",
                vec![
                    "--server",
                    &format!("https://{}/api", server_address),
                    "--port",
                    "47000",
                    "--transport-port",
                    "47001",
                    "--profile",
                    &format!(".run/Kokoverse/servers/{}/profiles/{}/profile.dat", server_name_hash, profile_name).to_string(),
                    "--data-dir",
                    &format!(".run/online/{}", profile_name).to_string(),
                    "--asset-dir",
                    &format!(".run/online/{}/assets", profile_name).to_string(),
                    "--adapter",
                    if cfg!(windows) {
                        "./.tools/kokosync-adapter.exe"
                    } else {
                        "./.tools/kokosync-adapter"
                    },
                    "--session",
                    &format!("{}", session_name).to_string(),
                ],
            );
        },
    }

    supervisor.spawn(
        "gui",
        if cfg!(windows) {
            ".tools/godot/Godot_v4.7.2-stable_win64"
        } else {
            "godot"
        },
        vec![
            "--path",
            "client-godot",
            "--",
            "--port=47000",
            &format!("--session={}", session_name).to_string(),
            "--close-with-engine",
        ],
    );
}
