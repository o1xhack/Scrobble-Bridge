use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use directories::ProjectDirs;
use thiserror::Error;

const DEFAULT_PORT: u16 = 8787;
const DEFAULT_INTERVAL_SECONDS: u64 = 600;
const MINIMUM_INTERVAL_SECONDS: u64 = 300;

#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub data_dir: PathBuf,
    pub web_dir: PathBuf,
    pub bind: SocketAddr,
    pub sync_interval: Duration,
    pub master_key_file: PathBuf,
    pub admin_token_file: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine an application data directory")]
    MissingDataDirectory,
    #[error("SCROBBLE_BIND is not a valid socket address: {0}")]
    InvalidBind(#[from] std::net::AddrParseError),
    #[error("SCROBBLE_SYNC_INTERVAL_SECONDS must be an integer")]
    InvalidInterval,
    #[error("SCROBBLE_SYNC_INTERVAL_SECONDS must be at least 300")]
    IntervalTooShort,
}

impl DaemonConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let data_dir = env::var_os("SCROBBLE_DATA_DIR")
            .map_or_else(default_data_dir, |path| Ok(PathBuf::from(path)))?;
        let bind = env::var("SCROBBLE_BIND")
            .unwrap_or_else(|_| {
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT).to_string()
            })
            .parse()?;
        let interval_seconds = env::var("SCROBBLE_SYNC_INTERVAL_SECONDS")
            .map_or(Ok(DEFAULT_INTERVAL_SECONDS), |value| {
                value.parse().map_err(|_| ConfigError::InvalidInterval)
            })?;
        if interval_seconds < MINIMUM_INTERVAL_SECONDS {
            return Err(ConfigError::IntervalTooShort);
        }
        let master_key_file = env::var_os("SCROBBLE_MASTER_KEY_FILE")
            .map_or_else(|| data_dir.join("secrets/master.key"), PathBuf::from);
        let admin_token_file = env::var_os("SCROBBLE_ADMIN_TOKEN_FILE")
            .map_or_else(|| data_dir.join("secrets/admin.token"), PathBuf::from);
        let web_dir = env::var_os("SCROBBLE_WEB_DIR")
            .map_or_else(|| PathBuf::from("apps/web/dist"), PathBuf::from);
        Ok(Self {
            data_dir,
            web_dir,
            bind,
            sync_interval: Duration::from_secs(interval_seconds),
            master_key_file,
            admin_token_file,
        })
    }
}

fn default_data_dir() -> Result<PathBuf, ConfigError> {
    ProjectDirs::from("com", "Scrobble Bridge", "Scrobble Bridge")
        .map(|directories| directories.data_local_dir().to_path_buf())
        .ok_or(ConfigError::MissingDataDirectory)
}
