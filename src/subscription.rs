use crate::config::{self, Subscription};
use anyhow::{anyhow, Context, Result};
use chrono::{TimeZone, Utc};

/// Downloaded subscription result.
pub struct FetchOutcome {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: Option<chrono::DateTime<Utc>>,
    #[allow(dead_code)]
    pub yaml_path: std::path::PathBuf,
}

pub async fn fetch(sub: &Subscription, user_agent: &str) -> Result<FetchOutcome> {
    fetch_with_proxy(sub, user_agent, None).await
}

/// If `proxy_url` is `Some("http://127.0.0.1:7890")`, the request goes through it (via mihomo).
/// If `None`, uses direct/no-proxy explicitly.
pub async fn fetch_with_proxy(
    sub: &Subscription,
    user_agent: &str,
    proxy_url: Option<&str>,
) -> Result<FetchOutcome> {
    config::ensure_dirs()?;

    // Handle local file subscriptions (imported via file://) by re-reading the source path.
    if let Some(path) = sub.url.strip_prefix("file://") {
        let text = tokio::fs::read_to_string(path).await
            .with_context(|| format!("read local subscription file {path}"))?;
        let v: serde_yaml::Value = serde_yaml::from_str(&text)
            .with_context(|| "local subscription file is not valid YAML")?;
        if !matches!(v, serde_yaml::Value::Mapping(_)) {
            return Err(anyhow!("subscription YAML root is not a mapping"));
        }
        let yaml_path = config::subscriptions_dir().join(format!("{}.yaml", sub.id));
        std::fs::write(&yaml_path, text).with_context(|| format!("write {:?}", yaml_path))?;
        return Ok(FetchOutcome {
            upload: 0,
            download: 0,
            total: 0,
            expire: None,
            yaml_path,
        });
    }

    let ua = if user_agent.trim().is_empty() {
        config::default_user_agent()
    } else {
        user_agent.to_string()
    };
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(ua);
    match proxy_url {
        Some(p) if !p.is_empty() => {
            let proxy = reqwest::Proxy::all(p).context("invalid proxy URL")?;
            builder = builder.proxy(proxy);
        }
        _ => {
            builder = builder.no_proxy();
        }
    }
    let client = builder.build()?;
    let resp = client.get(&sub.url).send().await.context("HTTP GET subscription")?;
    let resp = resp.error_for_status()?;

    let mut upload = 0u64;
    let mut download = 0u64;
    let mut total = 0u64;
    let mut expire: Option<chrono::DateTime<Utc>> = None;

    if let Some(hv) = resp.headers().get("subscription-userinfo")
        && let Ok(s) = hv.to_str() {
            for part in s.split(';').map(str::trim) {
                if let Some(v) = part.strip_prefix("upload=") {
                    upload = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = part.strip_prefix("download=") {
                    download = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = part.strip_prefix("total=") {
                    total = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = part.strip_prefix("expire=")
                    && let Ok(ts) = v.trim().parse::<i64>()
                        && ts > 0 {
                            expire = Utc.timestamp_opt(ts, 0).single();
                        }
            }
        }

    let text = resp.text().await?;

    // Sanity check: parseable as YAML mapping
    let v: serde_yaml::Value = serde_yaml::from_str(&text)
        .with_context(|| "subscription content is not valid YAML — check if URL returns a Clash config")?;
    if !matches!(v, serde_yaml::Value::Mapping(_)) {
        return Err(anyhow!("subscription YAML root is not a mapping"));
    }

    let yaml_path = config::subscriptions_dir().join(format!("{}.yaml", sub.id));
    std::fs::write(&yaml_path, text).with_context(|| format!("write {:?}", yaml_path))?;

    Ok(FetchOutcome {
        upload,
        download,
        total,
        expire,
        yaml_path,
    })
}

pub fn new_id() -> String {
    let now = chrono::Utc::now().timestamp_millis();
    format!("sub-{now}")
}

pub fn delete_local(id: &str) {
    let p = config::subscriptions_dir().join(format!("{}.yaml", id));
    let _ = std::fs::remove_file(p);
}
