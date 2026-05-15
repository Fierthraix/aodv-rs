use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, ValueEnum};
use serde::Deserialize;
use thiserror::Error;

use crate::AODV_PORT;

#[derive(Debug, Clone, Parser)]
#[command(name = "aodv", about = "AODV daemon implementing RFC 3561")]
pub struct CliArgs {
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(short = 'l', long)]
    pub local_ip: Option<Ipv4Addr>,

    #[arg(short = 'b', long)]
    pub bind_ip: Option<Ipv4Addr>,

    #[arg(short = 'B', long)]
    pub broadcast_ip: Option<Ipv4Addr>,

    #[arg(short, long)]
    pub port: Option<u16>,

    #[arg(short, long)]
    pub interface: Option<String>,

    #[arg(long)]
    pub disable_hello: bool,

    #[arg(short = 'm', long, value_enum)]
    pub data_plane: Option<DataPlaneMode>,

    #[arg(short = 'P', long)]
    pub data_port: Option<u16>,

    #[arg(short = 'n', long)]
    pub tun_name: Option<String>,

    #[arg(short = 't', long)]
    pub tun_ip: Option<Ipv4Addr>,

    #[arg(long)]
    pub tun_prefix: Option<u8>,

    #[arg(short = 'M', long)]
    pub tun_mtu: Option<u16>,

    #[arg(long)]
    pub route_metric: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataPlaneMode {
    ControlOnly,
    TunOverlay,
    KernelRoutes,
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
    pub data_plane: DataPlaneMode,
    pub data_port: u16,
    pub tun_name: String,
    pub tun_ip: Option<Ipv4Addr>,
    pub tun_prefix: u8,
    pub tun_mtu: u16,
    pub route_metric: u32,
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
        if let Some(data_plane) = args.data_plane {
            config.data_plane = data_plane;
        }
        if let Some(data_port) = args.data_port {
            config.data_port = data_port;
        }
        if let Some(tun_name) = args.tun_name {
            config.tun_name = tun_name;
        }
        if let Some(tun_ip) = args.tun_ip {
            config.tun_ip = Some(tun_ip);
        }
        if let Some(tun_prefix) = args.tun_prefix {
            config.tun_prefix = tun_prefix;
        }
        if let Some(tun_mtu) = args.tun_mtu {
            config.tun_mtu = tun_mtu;
        }
        if let Some(route_metric) = args.route_metric {
            config.route_metric = route_metric;
        }

        if config.local_ip == Ipv4Addr::UNSPECIFIED
            && config.data_plane == DataPlaneMode::TunOverlay
            && let Some(tun_ip) = config.tun_ip
        {
            config.local_ip = tun_ip;
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

    pub fn data_port(&self) -> u16 {
        self.data_port
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
        if let Some(value) = file.data_plane {
            self.data_plane = value;
        }
        if let Some(value) = file.data_port {
            self.data_port = value;
        }
        if let Some(value) = file.tun_name {
            self.tun_name = value;
        }
        if let Some(value) = file.tun_ip {
            self.tun_ip = Some(value);
        }
        if let Some(value) = file.tun_prefix {
            self.tun_prefix = value;
        }
        if let Some(value) = file.tun_mtu {
            self.tun_mtu = value;
        }
        if let Some(value) = file.route_metric {
            self.route_metric = value;
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
            data_plane: DataPlaneMode::ControlOnly,
            data_port: AODV_PORT + 1,
            tun_name: "aodv0".to_string(),
            tun_ip: None,
            tun_prefix: 24,
            tun_mtu: 1400,
            route_metric: 100,
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
    #[serde(default)]
    data_plane: Option<DataPlaneMode>,
    #[serde(default)]
    data_port: Option<u16>,
    #[serde(default)]
    tun_name: Option<String>,
    #[serde(default)]
    tun_ip: Option<Ipv4Addr>,
    #[serde(default)]
    tun_prefix: Option<u8>,
    #[serde(default)]
    tun_mtu: Option<u16>,
    #[serde(default)]
    route_metric: Option<u32>,
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

    // Keeps compatibility with the old YAML keys while proving the modern
    // Config defaults still fill in newer data-plane fields.
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
            data_plane: None,
            data_port: None,
            tun_name: None,
            tun_ip: None,
            tun_prefix: None,
            tun_mtu: None,
            route_metric: None,
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
        assert_eq!(config.data_plane, DataPlaneMode::ControlOnly);
        assert_eq!(config.data_port, AODV_PORT + 1);
        assert_eq!(config.tun_name, "aodv0");
        assert_eq!(config.tun_ip, None);
        assert_eq!(config.tun_prefix, 24);
        assert_eq!(config.tun_mtu, 1400);
        assert_eq!(config.route_metric, 100);
        assert_eq!(config.delete_period(), Duration::from_millis(15_005));
        assert_eq!(config.net_traversal_time(), Duration::from_millis(2_952));
        assert_eq!(config.path_discovery_time(), Duration::from_millis(5_904));
        assert_eq!(config.ring_traversal_time(0), Duration::from_millis(246));
    }

    // Covers the data-plane configuration merge: file values provide defaults,
    // and explicit CLI arguments take precedence over the file.
    #[test]
    fn parses_data_plane_options() {
        let file = NamedTempFile::new().unwrap();
        fs::write(
            file.path(),
            r#"data_plane: tun-overlay
data_port: 1655
tun_name: mesh0
tun_ip: 10.10.0.1
tun_prefix: 16
tun_mtu: 1280
route_metric: 77
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
            data_plane: Some(DataPlaneMode::KernelRoutes),
            data_port: None,
            tun_name: None,
            tun_ip: Some(Ipv4Addr::new(10, 20, 0, 1)),
            tun_prefix: None,
            tun_mtu: None,
            route_metric: None,
        };
        let config = Config::from_args(args).unwrap();

        assert_eq!(config.data_plane, DataPlaneMode::KernelRoutes);
        assert_eq!(config.data_port, 1655);
        assert_eq!(config.tun_name, "mesh0");
        assert_eq!(config.tun_ip, Some(Ipv4Addr::new(10, 20, 0, 1)));
        assert_eq!(config.tun_prefix, 16);
        assert_eq!(config.tun_mtu, 1280);
        assert_eq!(config.route_metric, 77);
    }

    #[test]
    fn tun_overlay_uses_tun_ip_as_default_local_ip() {
        let args = CliArgs {
            config: None,
            local_ip: None,
            bind_ip: Some(Ipv4Addr::new(192, 0, 2, 10)),
            broadcast_ip: None,
            port: None,
            interface: None,
            disable_hello: false,
            data_plane: Some(DataPlaneMode::TunOverlay),
            data_port: None,
            tun_name: None,
            tun_ip: Some(Ipv4Addr::new(10, 10, 0, 1)),
            tun_prefix: Some(16),
            tun_mtu: None,
            route_metric: None,
        };

        let config = Config::from_args(args).unwrap();

        assert_eq!(config.bind_ip, Ipv4Addr::new(192, 0, 2, 10));
        assert_eq!(config.local_ip, Ipv4Addr::new(10, 10, 0, 1));
    }

    #[test]
    fn parses_common_short_flags() {
        let args = CliArgs::try_parse_from([
            "aodv",
            "-l",
            "10.10.0.1",
            "-b",
            "192.0.2.10",
            "-B",
            "192.0.2.255",
            "-p",
            "654",
            "-i",
            "wlan0",
            "-m",
            "tun-overlay",
            "-P",
            "1655",
            "-n",
            "mesh0",
            "-t",
            "10.10.0.1",
            "-M",
            "1280",
        ])
        .unwrap();

        assert_eq!(args.local_ip, Some(Ipv4Addr::new(10, 10, 0, 1)));
        assert_eq!(args.bind_ip, Some(Ipv4Addr::new(192, 0, 2, 10)));
        assert_eq!(args.broadcast_ip, Some(Ipv4Addr::new(192, 0, 2, 255)));
        assert_eq!(args.port, Some(654));
        assert_eq!(args.interface.as_deref(), Some("wlan0"));
        assert_eq!(args.data_plane, Some(DataPlaneMode::TunOverlay));
        assert_eq!(args.data_port, Some(1655));
        assert_eq!(args.tun_name.as_deref(), Some("mesh0"));
        assert_eq!(args.tun_ip, Some(Ipv4Addr::new(10, 10, 0, 1)));
        assert_eq!(args.tun_mtu, Some(1280));
    }
}
