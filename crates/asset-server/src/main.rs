use std::path::PathBuf;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let arg = |name: &str, default: &str| {
        args.windows(2)
            .find(|w| w[0] == name)
            .map(|w| w[1].clone())
            .unwrap_or(default.into())
    };
    let addr: std::net::SocketAddr = arg("--listen", "127.0.0.1:8787").parse()?;
    if !addr.ip().is_loopback() {
        return Err("This prototype listens only on loopback; put a configured HTTPS gateway in front of it for remote use".into());
    }
    let server = kokoverse_asset_server::Server::open(
        PathBuf::from(arg("--data-dir", ".run/service")),
        arg("--relay", "127.0.0.1:48001"),
        arg("--quota-bytes", "1073741824").parse()?,
    )
    .map_err(|e| e.1)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("tukurium service http://{}", listener.local_addr()?);
    // Expired records are swept at startup (Server::open) and then hourly.
    let sweeper = server.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(3600));
        timer.tick().await;
        loop {
            timer.tick().await;
            match sweeper.sweep_expired() {
                Ok(n) if n > 0 => println!("Catalog sweep removed {n} expired records"),
                Ok(_) => {}
                Err(e) => eprintln!("Catalog sweep failed: {}", e.1),
            }
        }
    });
    axum::serve(listener, server.router())
        .with_graceful_shutdown(async {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("install SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
