use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const APP_ID: &str = "io.langya.ClashGNOME";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntervalUnit {
    Minutes,
    Hours,
    Days,
}

impl IntervalUnit {
    pub fn to_minutes(self, value: u32) -> u64 {
        match self {
            IntervalUnit::Minutes => value as u64,
            IntervalUnit::Hours => value as u64 * 60,
            IntervalUnit::Days => value as u64 * 60 * 24,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            IntervalUnit::Minutes => "minutes",
            IntervalUnit::Hours => "hours",
            IntervalUnit::Days => "days",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            IntervalUnit::Minutes => "m",
            IntervalUnit::Hours => "h",
            IntervalUnit::Days => "d",
        }
    }
}

impl Default for IntervalUnit {
    fn default() -> Self { IntervalUnit::Hours }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub expire: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default = "default_auto_update_value")]
    pub auto_update_value: u32,
    #[serde(default)]
    pub auto_update_unit: IntervalUnit,
    #[serde(default)]
    pub use_proxy_for_update: bool,
}

fn default_auto_update_value() -> u32 { 24 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_mihomo_path")]
    pub mihomo_path: String,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_api_host")]
    pub api_host: String,
    #[serde(default)]
    pub api_secret: String,
    #[serde(default = "default_mixed_port")]
    pub mixed_port: u16,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_true")]
    pub allow_lan: bool,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub tun_enabled: bool,
    #[serde(default = "default_tun_stack")]
    pub tun_stack: String,
    #[serde(default = "default_true")]
    pub tun_auto_route: bool,
    #[serde(default = "default_true")]
    pub tun_auto_detect_interface: bool,
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub start_kernel_on_launch: bool,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default)]
    pub active_subscription: Option<String>,
    #[serde(default = "default_dns_enable")]
    pub dns_enable: bool,
    #[serde(default = "default_dns_listen")]
    pub dns_listen: String,
    #[serde(default = "default_dns_enhanced")]
    pub dns_enhanced_mode: String,
    #[serde(default = "default_dns_nameservers")]
    pub dns_nameservers: Vec<String>,
    #[serde(default = "default_dns_fallback")]
    pub dns_fallback: Vec<String>,
    #[serde(default = "default_user_agent")]
    pub subscription_user_agent: String,
    #[serde(default = "default_true")]
    pub ipv6: bool,
    #[serde(default = "default_log_max_size")]
    pub log_max_size_mb: u32,
    #[serde(default = "default_log_max_days")]
    pub log_max_days: u32,
}

fn default_log_max_size() -> u32 { 20 }
fn default_log_max_days() -> u32 { 7 }

pub fn default_user_agent() -> String {
    format!("mihomo.gnome/v{} (clash.meta)", env!("CARGO_PKG_VERSION"))
}

fn default_mihomo_path() -> String {
    // Prefer PATH-resolved binary
    for candidate in ["/usr/bin/mihomo", "/usr/local/bin/mihomo", "/opt/mihomo/mihomo"] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    "mihomo".to_string()
}
fn default_api_port() -> u16 { 9090 }
fn default_api_host() -> String { "127.0.0.1".to_string() }
fn default_mixed_port() -> u16 { 7890 }
fn default_socks_port() -> u16 { 7891 }
fn default_http_port() -> u16 { 7892 }
fn default_true() -> bool { true }
fn default_mode() -> String { "rule".to_string() }
fn default_log_level() -> String { "info".to_string() }
fn default_tun_stack() -> String { "system".to_string() }
fn default_dns_enable() -> bool { true }
fn default_dns_listen() -> String { "0.0.0.0:53".to_string() }
fn default_dns_enhanced() -> String { "fake-ip".to_string() }
fn default_dns_nameservers() -> Vec<String> {
    vec![
        "https://1.1.1.1/dns-query".to_string(),
        "https://8.8.8.8/dns-query".to_string(),
    ]
}
fn default_dns_fallback() -> Vec<String> {
    vec![
        "tls://8.8.4.4:853".to_string(),
        "tls://1.0.0.1:853".to_string(),
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            mihomo_path: default_mihomo_path(),
            api_port: default_api_port(),
            api_host: default_api_host(),
            api_secret: String::new(),
            mixed_port: default_mixed_port(),
            socks_port: default_socks_port(),
            http_port: default_http_port(),
            allow_lan: true,
            mode: default_mode(),
            log_level: default_log_level(),
            tun_enabled: false,
            tun_stack: default_tun_stack(),
            tun_auto_route: true,
            tun_auto_detect_interface: true,
            theme: ThemeMode::default(),
            autostart: false,
            start_kernel_on_launch: false,
            subscriptions: Vec::new(),
            active_subscription: None,
            dns_enable: default_dns_enable(),
            dns_listen: default_dns_listen(),
            dns_enhanced_mode: default_dns_enhanced(),
            dns_nameservers: default_dns_nameservers(),
            dns_fallback: default_dns_fallback(),
            subscription_user_agent: default_user_agent(),
            ipv6: true,
            log_max_size_mb: default_log_max_size(),
            log_max_days: default_log_max_days(),
        }
    }
}

impl AppConfig {
    pub fn api_base(&self) -> String {
        format!("http://{}:{}", self.api_host, self.api_port)
    }
}

pub fn config_dir() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("clash-gnome");
    p
}

pub fn data_dir() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("clash-gnome");
    p
}

pub fn subscriptions_dir() -> PathBuf {
    let mut p = data_dir();
    p.push("subscriptions");
    p
}

pub fn mihomo_work_dir() -> PathBuf {
    let mut p = data_dir();
    p.push("mihomo");
    p
}

fn config_file() -> PathBuf {
    let mut p = config_dir();
    p.push("config.json");
    p
}

pub fn ensure_dirs() -> Result<()> {
    for d in [config_dir(), data_dir(), subscriptions_dir(), mihomo_work_dir()] {
        std::fs::create_dir_all(&d).with_context(|| format!("create dir {:?}", d))?;
    }
    Ok(())
}

pub fn load() -> AppConfig {
    let path = config_file();
    if !path.exists() {
        return AppConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            log::warn!("failed to parse config, using defaults: {e}");
            AppConfig::default()
        }),
        Err(e) => {
            log::warn!("failed to read config: {e}");
            AppConfig::default()
        }
    }
}

pub fn save(cfg: &AppConfig) -> Result<()> {
    ensure_dirs()?;
    let path = config_file();
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub type SharedConfig = Arc<RwLock<AppConfig>>;

pub fn shared() -> SharedConfig {
    Arc::new(RwLock::new(load()))
}

pub fn persist(shared: &SharedConfig) -> Result<()> {
    let g = shared.read().unwrap();
    save(&g)
}
