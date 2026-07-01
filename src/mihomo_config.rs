use crate::config::{self, AppConfig};
use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::path::{Path, PathBuf};

/// Generate the effective mihomo config.yaml by merging the user's app settings
/// with the currently-active subscription YAML (if any). Returns the path.
pub fn build_and_write(cfg: &AppConfig) -> Result<PathBuf> {
    config::ensure_dirs()?;
    let mut base = load_active_subscription(cfg).unwrap_or_else(|_| Mapping::new());

    apply_overrides(&mut base, cfg);

    let out_path = config::mihomo_work_dir().join("config.yaml");
    let yaml = serde_yaml::to_string(&Value::Mapping(base))?;
    std::fs::write(&out_path, yaml).with_context(|| format!("write {:?}", out_path))?;
    Ok(out_path)
}

pub fn active_subscription_path(cfg: &AppConfig) -> Option<PathBuf> {
    let id = cfg.active_subscription.as_ref()?;
    let p = config::subscriptions_dir().join(format!("{}.yaml", id));
    if p.exists() { Some(p) } else { None }
}

fn load_active_subscription(cfg: &AppConfig) -> Result<Mapping> {
    let Some(path) = active_subscription_path(cfg) else {
        return Ok(Mapping::new());
    };
    load_yaml_mapping(&path)
}

fn load_yaml_mapping(path: &Path) -> Result<Mapping> {
    let text = std::fs::read_to_string(path)?;
    let v: Value = serde_yaml::from_str(&text)?;
    match v {
        Value::Mapping(m) => Ok(m),
        _ => Ok(Mapping::new()),
    }
}

fn apply_overrides(m: &mut Mapping, cfg: &AppConfig) {
    set(m, "mixed-port", cfg.mixed_port);
    set(m, "socks-port", cfg.socks_port);
    set(m, "port", cfg.http_port);
    set(m, "allow-lan", cfg.allow_lan);
    set(m, "mode", cfg.mode.clone());
    set(m, "log-level", cfg.log_level.clone());
    set(m, "external-controller", format!("{}:{}", cfg.api_host, cfg.api_port));
    if !cfg.api_secret.is_empty() {
        set(m, "secret", cfg.api_secret.clone());
    } else {
        m.remove(Value::String("secret".into()));
    }
    // IPv6 on by default for modern setups
    set(m, "ipv6", true);
    // Enable unified-delay for stable delay readings
    set(m, "unified-delay", true);
    // TCP concurrent
    set(m, "tcp-concurrent", true);
    // Geo DBs — mihomo defaults are fine; keep as-is unless subscription overrides
    // External UI: leave subscription-provided if present, otherwise omit.

    // TUN
    let mut tun = existing_mapping(m, "tun").unwrap_or_default();
    set(&mut tun, "enable", cfg.tun_enabled);
    set(&mut tun, "stack", cfg.tun_stack.clone());
    set(&mut tun, "auto-route", cfg.tun_auto_route);
    set(&mut tun, "auto-detect-interface", cfg.tun_auto_detect_interface);
    set(&mut tun, "device", "clash-gnome".to_string());
    if !tun.contains_key(Value::String("dns-hijack".into())) {
        tun.insert(
            Value::String("dns-hijack".into()),
            Value::Sequence(vec![Value::String("any:53".into())]),
        );
    }
    if !tun.contains_key(Value::String("mtu".into())) {
        set(&mut tun, "mtu", 9000u32);
    }
    m.insert(Value::String("tun".into()), Value::Mapping(tun));

    // DNS
    let mut dns = existing_mapping(m, "dns").unwrap_or_default();
    set(&mut dns, "enable", cfg.dns_enable);
    set(&mut dns, "listen", cfg.dns_listen.clone());
    set(&mut dns, "enhanced-mode", cfg.dns_enhanced_mode.clone());
    set(&mut dns, "ipv6", true);
    if !dns.contains_key(Value::String("nameserver".into())) {
        dns.insert(
            Value::String("nameserver".into()),
            Value::Sequence(cfg.dns_nameservers.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if !dns.contains_key(Value::String("fallback".into())) {
        dns.insert(
            Value::String("fallback".into()),
            Value::Sequence(cfg.dns_fallback.iter().map(|s| Value::String(s.clone())).collect()),
        );
    }
    if !dns.contains_key(Value::String("fake-ip-range".into())) {
        set(&mut dns, "fake-ip-range", "198.18.0.1/16".to_string());
    }
    m.insert(Value::String("dns".into()), Value::Mapping(dns));

    // Profile
    let mut profile = existing_mapping(m, "profile").unwrap_or_default();
    set(&mut profile, "store-selected", true);
    set(&mut profile, "store-fake-ip", true);
    m.insert(Value::String("profile".into()), Value::Mapping(profile));
}

fn set<V: Into<Value>>(m: &mut Mapping, key: &str, v: V) {
    m.insert(Value::String(key.into()), v.into());
}

fn existing_mapping(m: &Mapping, key: &str) -> Option<Mapping> {
    match m.get(Value::String(key.into()))? {
        Value::Mapping(x) => Some(x.clone()),
        _ => None,
    }
}
