use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Api {
    base: String,
    secret: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    #[serde(default)]
    pub meta: bool,
    #[serde(default)]
    pub premium: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyItem {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub all: Option<Vec<String>>,
    #[serde(default)]
    pub udp: bool,
    #[serde(default)]
    pub history: Vec<DelayHistory>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DelayHistory {
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub delay: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxiesResponse {
    pub proxies: HashMap<String, ProxyItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleItem {
    #[serde(rename = "type")]
    pub ty: String,
    pub payload: String,
    pub proxy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RulesResponse {
    pub rules: Vec<RuleItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConnectionMeta {
    #[serde(default)]
    pub network: String,
    #[serde(default, rename = "type")]
    pub ty: String,
    #[serde(default, rename = "sourceIP")]
    pub source_ip: String,
    #[serde(default, rename = "destinationIP")]
    pub destination_ip: String,
    #[serde(default, rename = "sourcePort")]
    pub source_port: String,
    #[serde(default, rename = "destinationPort")]
    pub destination_port: String,
    #[serde(default)]
    pub host: String,
    #[serde(default, rename = "processPath")]
    pub process_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionItem {
    pub id: String,
    #[serde(default)]
    pub metadata: ConnectionMeta,
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub rule: String,
    #[serde(default, rename = "rulePayload")]
    pub rule_payload: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionsResponse {
    #[serde(default)]
    pub download_total: u64,
    #[serde(default)]
    pub upload_total: u64,
    #[serde(default)]
    pub connections: Vec<ConnectionItem>,
    #[serde(default)]
    pub memory: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrafficEvent {
    pub up: u64,
    pub down: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryEvent {
    pub inuse: u64,
    #[serde(default)]
    pub oslimit: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogEvent {
    #[serde(rename = "type", default)]
    pub level: String,
    #[serde(default)]
    pub payload: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigStatus {
    #[serde(default)]
    pub port: u16,
    #[serde(default, rename = "socks-port")]
    pub socks_port: u16,
    #[serde(default, rename = "mixed-port")]
    pub mixed_port: u16,
    #[serde(default, rename = "allow-lan")]
    pub allow_lan: bool,
    #[serde(default)]
    pub mode: String,
    #[serde(default, rename = "log-level")]
    pub log_level: String,
    #[serde(default)]
    pub ipv6: bool,
}

impl Api {
    pub fn new(base: impl Into<String>, secret: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("build reqwest client");
        Api {
            base: base.into(),
            secret: secret.into(),
            client,
        }
    }

    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base, path);
        let mut b = self.client.request(method, url);
        if !self.secret.is_empty() {
            b = b.bearer_auth(&self.secret);
        }
        b
    }

    pub async fn version(&self) -> Result<VersionInfo> {
        let r = self.req(reqwest::Method::GET, "/version").send().await?;
        Ok(r.error_for_status()?.json().await?)
    }

    pub async fn configs(&self) -> Result<ConfigStatus> {
        let r = self.req(reqwest::Method::GET, "/configs").send().await?;
        Ok(r.error_for_status()?.json().await?)
    }

    /// PATCH /configs — partial update: mode, log-level, allow-lan, ports, ipv6, tun, etc.
    pub async fn patch_configs(&self, patch: serde_json::Value) -> Result<()> {
        let r = self
            .req(reqwest::Method::PATCH, "/configs")
            .json(&patch)
            .send()
            .await?;
        r.error_for_status()?;
        Ok(())
    }

    /// PUT /configs?force=true — reload configuration from a file path (mihomo-side path)
    pub async fn reload_config(&self, path: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            path: &'a str,
        }
        let r = self
            .req(reqwest::Method::PUT, "/configs?force=true")
            .json(&Body { path })
            .send()
            .await?;
        r.error_for_status()?;
        Ok(())
    }

    pub async fn proxies(&self) -> Result<ProxiesResponse> {
        let r = self.req(reqwest::Method::GET, "/proxies").send().await?;
        Ok(r.error_for_status()?.json().await?)
    }

    pub async fn select_proxy(&self, group: &str, name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            name: &'a str,
        }
        let path = format!("/proxies/{}", urlencode(group));
        let r = self
            .req(reqwest::Method::PUT, &path)
            .json(&Body { name })
            .send()
            .await?;
        r.error_for_status()?;
        Ok(())
    }

    pub async fn test_delay(&self, name: &str, test_url: &str, timeout_ms: u32) -> Result<u32> {
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={}",
            urlencode(name),
            urlencode(test_url),
            timeout_ms
        );
        let r = self.req(reqwest::Method::GET, &path).send().await?;
        let status = r.status();
        if !status.is_success() {
            return Err(anyhow!("delay test failed: HTTP {}", status));
        }
        #[derive(Deserialize)]
        struct D {
            #[serde(default)]
            delay: u32,
        }
        let d: D = r.json().await?;
        Ok(d.delay)
    }

    pub async fn test_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u32,
    ) -> Result<HashMap<String, u32>> {
        let path = format!(
            "/group/{}/delay?url={}&timeout={}",
            urlencode(group),
            urlencode(test_url),
            timeout_ms
        );
        let r = self.req(reqwest::Method::GET, &path).send().await?;
        r.error_for_status()?
            .json::<HashMap<String, u32>>()
            .await
            .context("parse group delay")
    }

    pub async fn rules(&self) -> Result<RulesResponse> {
        let r = self.req(reqwest::Method::GET, "/rules").send().await?;
        Ok(r.error_for_status()?.json().await?)
    }

    pub async fn connections(&self) -> Result<ConnectionsResponse> {
        let r = self.req(reqwest::Method::GET, "/connections").send().await?;
        Ok(r.error_for_status()?.json().await?)
    }

    pub async fn close_all_connections(&self) -> Result<()> {
        let r = self.req(reqwest::Method::DELETE, "/connections").send().await?;
        r.error_for_status()?;
        Ok(())
    }

    pub async fn close_connection(&self, id: &str) -> Result<()> {
        let path = format!("/connections/{}", urlencode(id));
        let r = self.req(reqwest::Method::DELETE, &path).send().await?;
        r.error_for_status()?;
        Ok(())
    }

    pub async fn flush_fake_ip(&self) -> Result<()> {
        let r = self.req(reqwest::Method::POST, "/cache/fakeip/flush").send().await?;
        r.error_for_status()?;
        Ok(())
    }

    pub async fn upgrade_geo(&self) -> Result<()> {
        let r = self.req(reqwest::Method::POST, "/configs/geo").send().await?;
        r.error_for_status()?;
        Ok(())
    }

    /// POST /upgrade — ask the running mihomo to self-update. The core downloads
    /// a new binary into `<work-dir>/meta-update/` and exec-replaces itself.
    /// Connection typically drops mid-response; treat "connection reset" as success.
    pub async fn upgrade_core(&self) -> Result<()> {
        let r = self.req(reqwest::Method::POST, "/upgrade")
            .timeout(std::time::Duration::from_secs(90))
            .send()
            .await;
        match r {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("/upgrade returned HTTP {status}"))
                }
            }
            Err(e) if e.is_connect() || e.is_request() || e.is_body() || e.is_decode() => {
                // mihomo often kills the connection as it exec-replaces itself.
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Stream traffic events (server sends one JSON per line via chunked encoding).
    /// Returns a cancel sender: drop or send () to stop the streaming task.
    pub fn stream_traffic(self: &Arc<Self>) -> (mpsc::UnboundedReceiver<TrafficEvent>, mpsc::Sender<()>) {
        let (tx, rx) = mpsc::unbounded_channel::<TrafficEvent>();
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        let this = self.clone();
        tokio::spawn(async move {
            let resp = match this.req(reqwest::Method::GET, "/traffic").send().await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("traffic stream failed: {e}");
                    return;
                }
            };
            let mut stream = resp.bytes_stream();
            let mut buf = Vec::<u8>::new();
            loop {
                tokio::select! {
                    _ = cancel_rx.recv() => break,
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                buf.extend_from_slice(&bytes);
                                while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                                    let line = buf.drain(..=pos).collect::<Vec<u8>>();
                                    let line = &line[..line.len()-1];
                                    if line.is_empty() { continue; }
                                    if let Ok(ev) = serde_json::from_slice::<TrafficEvent>(line)
                                        && tx.send(ev).is_err() { return; }
                                }
                            }
                            Some(Err(e)) => { log::debug!("traffic stream err: {e}"); break; }
                            None => break,
                        }
                    }
                }
            }
        });
        (rx, cancel_tx)
    }

    pub fn stream_memory(self: &Arc<Self>) -> (mpsc::UnboundedReceiver<MemoryEvent>, mpsc::Sender<()>) {
        let (tx, rx) = mpsc::unbounded_channel::<MemoryEvent>();
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        let this = self.clone();
        tokio::spawn(async move {
            let resp = match this.req(reqwest::Method::GET, "/memory").send().await {
                Ok(r) => r,
                Err(_) => return,
            };
            let mut stream = resp.bytes_stream();
            let mut buf = Vec::<u8>::new();
            loop {
                tokio::select! {
                    _ = cancel_rx.recv() => break,
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                buf.extend_from_slice(&bytes);
                                while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                                    let line = buf.drain(..=pos).collect::<Vec<u8>>();
                                    let line = &line[..line.len()-1];
                                    if line.is_empty() { continue; }
                                    if let Ok(ev) = serde_json::from_slice::<MemoryEvent>(line)
                                        && tx.send(ev).is_err() { return; }
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }
        });
        (rx, cancel_tx)
    }

    pub fn stream_logs(self: &Arc<Self>, level: &str) -> (mpsc::UnboundedReceiver<LogEvent>, mpsc::Sender<()>) {
        let (tx, rx) = mpsc::unbounded_channel::<LogEvent>();
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        let this = self.clone();
        let path = format!("/logs?level={}", urlencode(level));
        tokio::spawn(async move {
            let resp = match this.req(reqwest::Method::GET, &path).send().await {
                Ok(r) => r,
                Err(_) => return,
            };
            let mut stream = resp.bytes_stream();
            let mut buf = Vec::<u8>::new();
            loop {
                tokio::select! {
                    _ = cancel_rx.recv() => break,
                    chunk = stream.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                buf.extend_from_slice(&bytes);
                                while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                                    let line = buf.drain(..=pos).collect::<Vec<u8>>();
                                    let line = &line[..line.len()-1];
                                    if line.is_empty() { continue; }
                                    if let Ok(ev) = serde_json::from_slice::<LogEvent>(line)
                                        && tx.send(ev).is_err() { return; }
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }
        });
        (rx, cancel_tx)
    }
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
