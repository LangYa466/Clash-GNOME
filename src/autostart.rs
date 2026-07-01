use anyhow::{Context, Result};
use std::path::PathBuf;

fn autostart_dir() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("autostart");
    p
}

fn autostart_file() -> PathBuf {
    autostart_dir().join("io.github.langya.ClashGNOME.desktop")
}

fn current_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "clash-gnome".to_string())
}

pub fn is_enabled() -> bool {
    autostart_file().exists()
}

pub fn enable() -> Result<()> {
    std::fs::create_dir_all(autostart_dir()).context("create autostart dir")?;
    let exe = current_exe();
    let content = format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name=Clash GNOME\n\
Comment=Modern GNOME frontend for mihomo\n\
Exec={exe} --hidden\n\
Icon=io.github.langya.ClashGNOME\n\
Terminal=false\n\
Categories=Network;\n\
X-GNOME-Autostart-enabled=true\n\
StartupNotify=false\n"
    );
    std::fs::write(autostart_file(), content).context("write autostart .desktop")?;
    Ok(())
}

pub fn disable() -> Result<()> {
    let p = autostart_file();
    if p.exists() {
        std::fs::remove_file(&p).context("remove autostart .desktop")?;
    }
    Ok(())
}

pub fn set(enabled: bool) -> Result<()> {
    if enabled { enable() } else { disable() }
}
