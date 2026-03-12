use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use serde::Deserialize;
use thiserror::Error;

use crate::AODV_PORT;

#[derive(Debug, Clone, Parser)]
#[command(name = "aodv", about = "AODV daemon implementing RFC 3561")]
pub struct CliArgs {
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub local_ip: Option<Ipv4Addr>,

    #[arg(long)]
    pub bind_ip: Option<Ipv4Addr>,

    #[arg(long)]
    pub broadcast_ip: Option<Ipv4Addr>,

    #[arg(short, long)]
    pub port: Option<u16>,

    #[arg(long)]
    pub interface: Option<String>,

    #[arg(long)]
    pub disable_hello: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub local_ip: Ipv4Addr,
    pub bind_ip: Ipv4Addr,
    pub broadcast_ip: Ipv4Addr,
    pub port: u16,
    pub interface: Option<String>,
    pub enable_hello: bool,
    pub active_route_timeout: Duration,
    pub allowed_hello_loss: u32,
    pub hello_interval: Duration,
    pub local_add_ttl: u8,
    pub net_diameter: u8,
    pub node_traversal_time: Duration,
    pub rerr_ratelimit: usize,
    pub rreq_retries: usize,
    pub rreq_ratelimit: usize,
    pub timeout_buffer: u8,
    pub ttl_start: u8,
    pub ttl_increment: u8,
    pub ttl_threshold: u8,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    ReadConfig {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    ParseConfig {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
}

impl Config {
    pub fn from_cli() -> Result<Self, ConfigError> {
        Self::from_args(CliArgs::parse())
    }

    pub fn from_args(args: CliArgs) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        if let Some(path) = args.config.as_deref() {
            let file_config = FileConfig::from_path(path)?;
            config.apply_file_config(file_config);
        }

        if let Some(local_ip) = args.local_ip {
            config.local_ip = local_ip;
        }
        if let Some(bind_ip) = args.bind_ip {
            config.bind_ip = bind_ip;
        }
        if let Some(broadcast_ip) = args.broadcast_ip {
            config.broadcast_ip = broadcast_ip;
        }
        if let Some(port) = args.port {
            config.port = port;
        }
        if let Some(interface) = args.interface {
            config.interface = Some(interface);
        }
        if args.disable_hello {
            config.enable_hello = false;
        }

        if config.local_ip == Ipv4Addr::UNSPECIFIED {
            config.local_ip = config.bind_ip;
        }

        Ok(config)
    }

    pub fn delete_period(&self) -> Duration {
        duration_mul(self.active_route_timeout.max(self.hello_interval), 5)
    }

    pub fn max_repair_ttl(&self) -> u8 {
        ((self.net_diameter as f32) * 0.3).ceil() as u8
    }

    pub fn my_route_timeout(&self) -> Duration {
        duration_mul(self.active_route_timeout, 2)
    }

    pub fn net_traversal_time(&self) -> Duration {
        duration_mul(self.node_traversal_time, 2 * self.net_diameter as u32)
    }

    pub fn next_hop_wait(&self) -> Duration {
        self.node_traversal_time + Duration::from_millis(10)
    }

    pub fn path_discovery_time(&self) -> Duration {
        duration_mul(self.net_traversal_time(), 2)
    }

    pub fn ring_traversal_time(&self, ttl_value: u8) -> Duration {
        duration_mul(
            self.node_traversal_time,
            2 * (ttl_value as u32 + self.timeout_buffer as u32),
        )
    }

    pub fn blacklist_timeout(&self) -> Duration {
        duration_mul(self.net_traversal_time(), self.rreq_retries as u32)
    }

    pub fn hello_timeout(&self, advertised_interval_ms: Option<u32>) -> Duration {
        let interval = advertised_interval_ms
            .map(|ms| Duration::from_millis(ms as u64))
            .unwrap_or(self.hello_interval);
        duration_mul(interval, self.allowed_hello_loss)
    }

    pub fn aodv_port(&self) -> u16 {
        self.port
    }

    fn apply_file_config(&mut self, file: FileConfig) {
        if let Some(local_ip) = file.local_ip {
            self.local_ip = local_ip;
        }
        if let Some(bind_ip) = file.bind_ip {
            self.bind_ip = bind_ip;
        }
        if let Some(broadcast_ip) = file.broadcast_ip {
            self.broadcast_ip = broadcast_ip;
        }
        if let Some(port) = file.port {
            self.port = port;
        }
        if let Some(interface) = file.interface {
            self.interface = Some(interface);
        }
        if let Some(enable_hello) = file.enable_hello {
            self.enable_hello = enable_hello;
        }
        if let Some(value) = file.active_route_timeout_ms {
            self.active_route_timeout = Duration::from_millis(value);
        }
        if let Some(value) = file.allowed_hello_loss {
            self.allowed_hello_loss = value;
        }
        if let Some(value) = file.hello_interval_ms {
            self.hello_interval = Duration::from_millis(value);
        }
        if let Some(value) = file.local_add_ttl {
            self.local_add_ttl = value;
        }
        if let Some(value) = file.net_diameter {
            self.net_diameter = value;
        }
        if let Some(value) = file.node_traversal_time_ms {
            self.node_traversal_time = Duration::from_millis(value);
        }
        if let Some(value) = file.rerr_ratelimit {
            self.rerr_ratelimit = value;
        }
        if let Some(value) = file.rreq_retries {
            self.rreq_retries = value;
        }
        if let Some(value) = file.rreq_ratelimit {
            self.rreq_ratelimit = value;
        }
        if let Some(value) = file.timeout_buffer {
            self.timeout_buffer = value;
        }
        if let Some(value) = file.ttl_start {
            self.ttl_start = value;
        }
        if let Some(value) = file.ttl_increment {
            self.ttl_increment = value;
        }
        if let Some(value) = file.ttl_threshold {
            self.ttl_threshold = value;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            local_ip: Ipv4Addr::UNSPECIFIED,
            bind_ip: Ipv4Addr::UNSPECIFIED,
            broadcast_ip: Ipv4Addr::new(255, 255, 255, 255),
            port: AODV_PORT,
            interface: None,
            enable_hello: true,
            active_route_timeout: Duration::from_millis(3_000),
            allowed_hello_loss: 2,
            hello_interval: Duration::from_millis(1_000),
            local_add_ttl: 2,
            net_diameter: 35,
            node_traversal_time: Duration::from_millis(40),
            rerr_ratelimit: 10,
            rreq_retries: 2,
            rreq_ratelimit: 10,
            timeout_buffer: 2,
            ttl_start: 1,
            ttl_increment: 2,
            ttl_threshold: 7,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default, alias = "current_ip", alias = "CurrentIp")]
    local_ip: Option<Ipv4Addr>,
    #[serde(default)]
    bind_ip: Option<Ipv4Addr>,
    #[serde(
        default,
        alias = "broadcast_address",
        alias = "BroadcastAddress",
        alias = "broadcastAddress"
    )]
    broadcast_ip: Option<Ipv4Addr>,
    #[serde(default, alias = "Port")]
    port: Option<u16>,
    #[serde(default, alias = "Interface")]
    interface: Option<String>,
    #[serde(default)]
    enable_hello: Option<bool>,
    #[serde(default, alias = "ACTIVE_ROUTE_TIMEOUT")]
    active_route_timeout_ms: Option<u64>,
    #[serde(default, alias = "ALLOWED_HELLO_LOSS")]
    allowed_hello_loss: Option<u32>,
    #[serde(default, alias = "HELLO_INTERVAL")]
    hello_interval_ms: Option<u64>,
    #[serde(default, alias = "LOCAL_ADD_TTL")]
    local_add_ttl: Option<u8>,
    #[serde(default, alias = "NET_DIAMETER")]
    net_diameter: Option<u8>,
    #[serde(default, alias = "NODE_TRAVERSAL_TIME")]
    node_traversal_time_ms: Option<u64>,
    #[serde(default, alias = "RERR_RATELIMIT")]
    rerr_ratelimit: Option<usize>,
    #[serde(default, alias = "RREQ_RETRIES")]
    rreq_retries: Option<usize>,
    #[serde(default, alias = "RREQ_RATELIMIT")]
    rreq_ratelimit: Option<usize>,
    #[serde(default, alias = "TIMEOUT_BUFFER")]
    timeout_buffer: Option<u8>,
    #[serde(default, alias = "TTL_START")]
    ttl_start: Option<u8>,
    #[serde(default, alias = "TTL_INCREMENT")]
    ttl_increment: Option<u8>,
    #[serde(default, alias = "TTL_THRESHOLD")]
    ttl_threshold: Option<u8>,
}

impl FileConfig {
    fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let display = path.display().to_string();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::ReadConfig {
            path: display.clone(),
            source,
        })?;
        serde_yaml::from_str(&contents).map_err(|source| ConfigError::ParseConfig {
            path: display,
            source,
        })
    }
}

pub fn duration_mul(duration: Duration, factor: u32) -> Duration {
    Duration::from_millis(duration.as_millis().saturating_mul(factor as u128) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_legacy_yaml_config() {
        let file = NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            r#"Interface: "wlan1"
BroadcastAddress: "192.168.10.251"
Port: 1201
ACTIVE_ROUTE_TIMEOUT: 3001
ALLOWED_HELLO_LOSS: 3
HELLO_INTERVAL: 1001
LOCAL_ADD_TTL: 3
NET_DIAMETER: 36
NODE_TRAVERSAL_TIME: 41
RERR_RATELIMIT: 11
RREQ_RETRIES: 3
RREQ_RATELIMIT: 11
TIMEOUT_BUFFER: 3
TTL_START: 2
TTL_INCREMENT: 3
TTL_THRESHOLD: 8
"#,
        )
        .unwrap();

        let args = CliArgs {
            config: Some(file.path().to_path_buf()),
            local_ip: None,
            bind_ip: None,
            broadcast_ip: None,
            port: None,
            interface: None,
            disable_hello: false,
        };
        let config = Config::from_args(args).unwrap();

        assert_eq!(config.interface.as_deref(), Some("wlan1"));
        assert_eq!(config.broadcast_ip, Ipv4Addr::new(192, 168, 10, 251));
        assert_eq!(config.port, 1201);
        assert_eq!(config.active_route_timeout, Duration::from_millis(3001));
        assert_eq!(config.allowed_hello_loss, 3);
        assert_eq!(config.hello_interval, Duration::from_millis(1001));
        assert_eq!(config.local_add_ttl, 3);
        assert_eq!(config.net_diameter, 36);
        assert_eq!(config.node_traversal_time, Duration::from_millis(41));
        assert_eq!(config.rerr_ratelimit, 11);
        assert_eq!(config.rreq_retries, 3);
        assert_eq!(config.rreq_ratelimit, 11);
        assert_eq!(config.timeout_buffer, 3);
        assert_eq!(config.ttl_start, 2);
        assert_eq!(config.ttl_increment, 3);
        assert_eq!(config.ttl_threshold, 8);
        assert_eq!(config.delete_period(), Duration::from_millis(15_005));
        assert_eq!(config.net_traversal_time(), Duration::from_millis(2_952));
        assert_eq!(config.path_discovery_time(), Duration::from_millis(5_904));
        assert_eq!(config.ring_traversal_time(0), Duration::from_millis(246));
    }
}
