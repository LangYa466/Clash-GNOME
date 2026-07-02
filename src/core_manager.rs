use crate::api::Api;
use crate::config::{self, AppConfig, SharedConfig};
use crate::mihomo_config;
use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

pub struct CoreManager {
    child: Mutex<Option<Child>>,
    state: Mutex<CoreState>,
    log_tx: mpsc::UnboundedSender<String>,
    pub log_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    cfg: SharedConfig,
}

impl CoreManager {
    pub fn new(cfg: SharedConfig) -> Arc<Self> {
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        Arc::new(CoreManager {
            child: Mutex::new(None),
            state: Mutex::new(CoreState::Stopped),
            log_tx,
            log_rx: Mutex::new(Some(log_rx)),
            cfg,
        })
    }

    pub async fn state(&self) -> CoreState {
        *self.state.lock().await
    }

    pub async fn take_log_rx(&self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.log_rx.lock().await.take()
    }

    pub fn api(&self) -> Api {
        let cfg = self.cfg.read().unwrap();
        Api::new(cfg.api_base(), cfg.api_secret.clone())
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        {
            let s = *self.state.lock().await;
            if matches!(s, CoreState::Running | CoreState::Starting) {
                return Ok(());
            }
        }
        *self.state.lock().await = CoreState::Starting;

        let (mihomo_path, work_dir) = {
            let cfg = self.cfg.read().unwrap();
            let path = cfg.mihomo_path.clone();
            let work = config::mihomo_work_dir();
            (path, work)
        };

        // Generate config.yaml
        {
            let cfg = self.cfg.read().unwrap().clone();
            mihomo_config::build_and_write(&cfg).context("generate mihomo config")?;
        }

        config::ensure_dirs()?;

        let mut cmd = Command::new(&mihomo_path);
        cmd.arg("-d").arg(&work_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(Stdio::null());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().with_context(|| format!("spawn {}", mihomo_path))?;

        // Pipe stdout / stderr into log channel
        if let Some(stdout) = child.stdout.take() {
            let tx = self.log_tx.clone();
            tokio::spawn(async move {
                let mut r = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = r.next_line().await {
                    let _ = tx.send(line);
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let tx = self.log_tx.clone();
            tokio::spawn(async move {
                let mut r = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = r.next_line().await {
                    let _ = tx.send(format!("[stderr] {line}"));
                }
            });
        }

        *self.child.lock().await = Some(child);

        // Wait for API to become ready
        let api = self.api();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if std::time::Instant::now() > deadline {
                *self.state.lock().await = CoreState::Failed;
                let _ = self.stop().await;
                return Err(anyhow!("mihomo did not become ready within 10s"));
            }
            if api.version().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        *self.state.lock().await = CoreState::Running;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.state.lock().await = CoreState::Stopping;
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        *self.state.lock().await = CoreState::Stopped;
        Ok(())
    }

    pub async fn restart(self: &Arc<Self>) -> Result<()> {
        self.stop().await?;
        self.start().await
    }

    /// Update the mihomo binary. If the core is running, uses mihomo's own
    /// `POST /upgrade` and waits for the `<workdir>/meta-update` folder to
    /// disappear (signal that self-update finished). If the core is not
    /// running, downloads the latest release via `mihomo_download` and
    /// overwrites the managed binary at `mihomo_managed_path()`.
    ///
    /// Returns the installed version string on success.
    pub async fn upgrade_core(self: &Arc<Self>) -> Result<String> {
        let running = *self.state.lock().await == CoreState::Running;
        let github_proxy = self.cfg.read().unwrap().github_proxy.clone();

        if running {
            self.api().upgrade_core().await.context("POST /upgrade")?;

            // Watch for mihomo to finish self-update.
            let marker = config::mihomo_work_dir().join("meta-update");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            let mut saw = false;
            loop {
                let exists = marker.exists();
                if exists {
                    saw = true;
                }
                if saw && !exists {
                    break;
                }
                if std::time::Instant::now() > deadline {
                    if !saw {
                        // meta-update never appeared: mihomo probably already restarted.
                        break;
                    }
                    return Err(anyhow!("mihomo self-update timed out (meta-update not removed)"));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }

            // mihomo self-update exec-replaces the process; wait for API to come back.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let api = self.api();
            loop {
                if let Ok(v) = api.version().await {
                    *self.state.lock().await = CoreState::Running;
                    return Ok(format!("v{}", v.version));
                }
                if std::time::Instant::now() > deadline {
                    *self.state.lock().await = CoreState::Failed;
                    return Err(anyhow!("mihomo did not respond after self-update"));
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        }

        // Not running — download to the managed path and point config at it.
        let version = crate::mihomo_download::latest_version(&github_proxy).await
            .context("fetch latest mihomo version")?;
        let dest = crate::mihomo_download::install(&version, &github_proxy).await
            .context("install mihomo")?;
        {
            let mut cfg = self.cfg.write().unwrap();
            cfg.mihomo_path = dest.to_string_lossy().into_owned();
        }
        let _ = config::persist(&self.cfg);
        Ok(version)
    }

    /// Re-generate config.yaml from current AppConfig and ask mihomo to reload it in place.
    /// If the core is not running, just regenerates the file.
    pub async fn apply_config(&self) -> Result<()> {
        let path = {
            let cfg = self.cfg.read().unwrap().clone();
            mihomo_config::build_and_write(&cfg)?
        };
        if *self.state.lock().await == CoreState::Running {
            let api = self.api();
            api.reload_config(path.to_str().unwrap_or("")).await
                .context("mihomo reload config")?;
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub fn detect_binary(cfg: &AppConfig) -> Option<String> {
    let candidate = &cfg.mihomo_path;
    if std::path::Path::new(candidate).exists() {
        return Some(candidate.clone());
    }
    // Try which
    let which = std::process::Command::new("which").arg(candidate).output().ok()?;
    if which.status.success() {
        let p = String::from_utf8_lossy(&which.stdout).trim().to_string();
        if !p.is_empty() {
            return Some(p);
        }
    }
    None
}

pub async fn setcap_via_pkexec(binary: &str) -> Result<()> {
    let status = Command::new("pkexec")
        .arg("setcap")
        .arg("cap_net_admin,cap_net_bind_service,cap_dac_override,cap_sys_ptrace+eip")
        .arg(binary)
        .status()
        .await
        .context("pkexec setcap")?;
    if !status.success() {
        return Err(anyhow!("setcap failed with status {status}"));
    }
    Ok(())
}
