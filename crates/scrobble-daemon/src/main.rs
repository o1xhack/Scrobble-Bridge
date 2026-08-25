use scrobble_daemon::{Daemon, DaemonConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck().await;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();
    Daemon::open(DaemonConfig::from_env()?)?.serve().await?;
    Ok(())
}

async fn healthcheck() -> Result<(), Box<dyn std::error::Error>> {
    use tokio::{io::AsyncReadExt, io::AsyncWriteExt};

    let mut address = DaemonConfig::from_env()?.bind;
    if address.ip().is_unspecified() {
        address.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    }
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(b"GET /health/live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    if response.starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err("daemon liveness check did not return HTTP 200".into())
    }
}
