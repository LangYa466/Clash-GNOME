# Clash GNOME

A modern, native GTK4 + libadwaita desktop client for the [mihomo](https://github.com/MetaCubeX/mihomo) (Clash Meta) proxy core, written in Rust.

Not a web wrapper. No Electron. Follows the GNOME Human Interface Guidelines and adapts to your system's light / dark theme automatically.

## Features

- **Dashboard** — real-time upload / download speed, session totals, memory, active connection count
- **Proxy groups** — inspect all Selector / URLTest / Fallback / LoadBalance groups, one-click select, one-click test-all
- **Rules browser** — search and filter the entire ruleset served by the running kernel
- **Active connections** — live view with per-connection close, chain / rule / process metadata
- **Subscriptions** — import from URL **or local YAML file**, track quota (`subscription-userinfo`), update on demand, active toggle
- **Persistence** — all settings, subscriptions, ports, theme, TUN, active profile survive restarts (config at `~/.config/clash-gnome/config.json`)
- **Log stream** — live tail with level filter (info / warning / error / debug), colored output
- **Settings** — kernel path, ports, secret, TUN, DNS, allow-LAN, log level, theme, autostart
- **TUN mode** — transparent proxy with system / gvisor / mixed stack; helper to grant `cap_net_admin` via pkexec
- **Autostart** — XDG-compliant `~/.config/autostart/` entry with optional hidden launch
- **Adaptive theming** — follows GNOME light / dark, or force one via settings
- **Bundled mihomo core** — the `.deb` and tarball ship a matching-arch mihomo binary; the app also has one-click install/update from Settings with a configurable GitHub proxy

## Install

### Debian / Ubuntu / Zorin (recommended)

Grab the latest `.deb` for your arch from [Releases](https://github.com/LangYa466/Clash-GNOME/releases) and:

```bash
sudo apt install ./clash-gnome_*_amd64.deb    # or _arm64.deb
```

mihomo is bundled at `/usr/lib/clash-gnome/mihomo`, so nothing else to install. Launch Clash GNOME, import a subscription, click **Start Core**.

### Portable tarball

```bash
tar xzf clash-gnome-vX.Y.Z-x86_64-linux.tar.gz
cd clash-gnome-vX.Y.Z-x86_64-linux
sudo ./install.sh    # installs into /usr/local
```

### Build from source

Requirements: Rust 1.85+ (edition 2024), GTK4 ≥ 4.10, libadwaita ≥ 1.5.

```bash
# Debian/Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev libdbus-1-dev libssl-dev build-essential pkg-config
# Fedora
sudo dnf install gtk4-devel libadwaita-devel dbus-devel openssl-devel gcc pkgconf-pkg-config
# Arch
sudo pacman -S gtk4 libadwaita rust

cargo build --release
```

If you build from source, mihomo is not bundled — install it once from **Settings → mihomo core → Install**, or drop your own binary in `~/.local/share/clash-gnome/bin/mihomo`.

## First run

1. Launch Clash GNOME.
2. Go to **Subscriptions** → **Add Subscription** (paste a URL or pick a local `.yaml`).
3. Back to **Dashboard** → **Start Core**.
4. Point your system proxy at `http://127.0.0.1:7890` (mixed), or enable **TUN mode** for transparent capture.

### Updating the mihomo core

**Settings → mihomo core** shows the installed vs latest upstream version. Click **Update to vX.Y.Z** to upgrade — if the core is running it uses mihomo's own `POST /upgrade`, otherwise it downloads the release to `~/.local/share/clash-gnome/bin/mihomo`.

Behind the Great Firewall? Pick a **GitHub proxy** in the same group (`gh-proxy.org`, `ghfast.top`, `down.clashparty.org`, `download.mihomo.party`, or **Auto** to try each in order).

### Enabling TUN mode

TUN needs `cap_net_admin` on the mihomo binary. From **Settings → Grant TUN capabilities**, click **Run setcap** — this uses `pkexec` for one-time authorization.

## Storage layout

- App config: `~/.config/clash-gnome/config.json`
- Subscriptions: `~/.local/share/clash-gnome/subscriptions/<id>.yaml`
- Generated mihomo config: `~/.local/share/clash-gnome/mihomo/config.yaml`
- User-installed mihomo binary: `~/.local/share/clash-gnome/bin/mihomo`
- Bundled mihomo binary (`.deb` / tarball): `/usr/lib/clash-gnome/mihomo`
- Autostart: `~/.config/autostart/io.langya.ClashGNOME.desktop`

Every setting change (mode, ports, TUN toggle, theme, subscriptions, etc.) is written to `config.json` immediately, so all state survives an app restart.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
