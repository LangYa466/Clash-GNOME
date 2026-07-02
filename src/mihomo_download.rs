use crate::config;
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::PathBuf;

const GITHUB_PROXIES: &[&str] = &[
    "https://gh-proxy.org",
    "https://ghfast.top",
    "https://down.clashparty.org",
    "https://download.mihomo.party",
];

const VERSION_URL: &str = "https://github.com/MetaCubeX/mihomo/releases/latest/download/version.txt";
const RELEASE_PREFIX: &str = "https://github.com/MetaCubeX/mihomo/releases/download";

fn asset_name(version: &str) -> Result<String> {
    let arch = std::env::consts::ARCH;
    let stem = match arch {
        "x86_64" => "mihomo-linux-amd64-compatible",
        "aarch64" => "mihomo-linux-arm64",
        other => return Err(anyhow!("unsupported architecture: {other}")),
    };
    Ok(format!("{stem}-{version}.gz"))
}

fn build_urls(github_url: &str, proxy_pref: &str) -> Vec<String> {
    match proxy_pref {
        "" | "direct" => vec![github_url.to_string()],
        "auto" => {
            let mut v: Vec<String> = GITHUB_PROXIES
                .iter()
                .map(|p| format!("{p}/{github_url}"))
                .collect();
            v.push(github_url.to_string());
            v
        }
        specific => vec![format!("{}/{}", specific.trim_end_matches('/'), github_url)],
    }
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(config::default_user_agent())
        .no_proxy()
        .build()
        .map_err(Into::into)
}

async fn try_get_bytes(urls: &[String]) -> Result<Vec<u8>> {
    let c = client()?;
    let mut last = anyhow!("no URLs to try");
    for url in urls {
        match c.get(url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(b) => return Ok(b.to_vec()),
                Err(e) => last = anyhow!("read body from {url}: {e}"),
            },
            Err(e) => last = anyhow!("GET {url}: {e}"),
        }
    }
    Err(last)
}

/// Fetch the latest upstream release version string (e.g. "v1.19.10").
pub async fn latest_version(github_proxy: &str) -> Result<String> {
    let urls = build_urls(VERSION_URL, github_proxy);
    let bytes = try_get_bytes(&urls).await.context("fetch version.txt")?;
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    // Guard against proxies that return an HTML error page with 200.
    let looks_like_version = text
        .strip_prefix('v')
        .and_then(|s| s.split('.').next())
        .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
    if !looks_like_version {
        return Err(anyhow!("version.txt did not look like a mihomo version: {text:?}"));
    }
    Ok(text)
}

/// Download the release asset for the given version, gunzip it, install to
/// `mihomo_managed_path()`, chmod 755, and return the destination path.
pub async fn install(version: &str, github_proxy: &str) -> Result<PathBuf> {
    let asset = asset_name(version)?;
    let url = format!("{RELEASE_PREFIX}/{version}/{asset}");
    let urls = build_urls(&url, github_proxy);
    let gz = try_get_bytes(&urls).await.context("download mihomo asset")?;

    let mut decoder = GzDecoder::new(&gz[..]);
    let mut binary = Vec::with_capacity(gz.len() * 3);
    decoder.read_to_end(&mut binary).context("gunzip mihomo asset")?;
    if binary.len() < 1024 {
        return Err(anyhow!("decompressed binary is implausibly small ({} bytes)", binary.len()));
    }

    let dest = config::mihomo_managed_path();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {parent:?}"))?;
    }
    let tmp = dest.with_extension("tmp");
    tokio::fs::write(&tmp, &binary).await.with_context(|| format!("write {tmp:?}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("chmod {tmp:?}"))?;
    }
    std::fs::rename(&tmp, &dest).with_context(|| format!("rename {tmp:?} -> {dest:?}"))?;
    Ok(dest)
}

/// Ask an installed mihomo binary for its version by exec'ing `<path> -v`.
/// Returns e.g. "v1.19.10" or None if the binary is absent / unparseable.
pub async fn installed_version(binary: &str) -> Option<String> {
    let out = tokio::process::Command::new(binary)
        .arg("-v")
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for token in text.split_whitespace() {
        if let Some(rest) = token.strip_prefix('v')
            && rest.split('.').next().is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(token.to_string());
        }
    }
    None
}
