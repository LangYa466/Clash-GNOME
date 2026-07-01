use once_cell::sync::OnceCell;
use std::future::Future;
use tokio::runtime::Runtime;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

pub fn init_runtime() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .thread_name("clash-gnome-tokio")
        .build()
        .expect("failed to create tokio runtime");
    RUNTIME.set(rt).ok();
}

pub fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("runtime not initialized")
}

/// Spawn a future on tokio and deliver the result back to the GLib main loop.
pub fn spawn<F, T, C>(fut: F, on_done: C)
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
    C: FnOnce(T) + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let out = fut.await;
        let _ = tx.send(out).await;
    });
    glib::MainContext::default().spawn_local(async move {
        if let Ok(v) = rx.recv().await {
            on_done(v);
        }
    });
}

/// Fire-and-forget async task on tokio (no GLib callback).
pub fn detach<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    runtime().spawn(fut);
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

pub fn format_speed(n: u64) -> String {
    format!("{}/s", format_bytes(n))
}

/// Best-effort read of GNOME's system proxy mode via `gsettings`.
/// Returns "None"/"Manual"/"Auto"/"Unknown".
pub fn system_proxy_summary() -> String {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output();
    let raw = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return "Unknown".to_string(),
    };
    let cleaned = raw.trim_matches('\'').to_lowercase();
    match cleaned.as_str() {
        "none" => "Off".to_string(),
        "manual" => "Manual".to_string(),
        "auto" => "Auto (PAC)".to_string(),
        other => title(other),
    }
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
