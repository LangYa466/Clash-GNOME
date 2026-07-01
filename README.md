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

## Requirements

- Rust 1.85+ (edition 2024)
- GTK4 >= 4.10
- libadwaita >= 1.5
- mihomo binary installed (`sudo pacman -S mihomo` on Arch, or build from [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo/releases))
- pkg-config, glib, cairo, pango, gdk-pixbuf, graphene, openssl development headers

On Debian/Ubuntu:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev build-essential pkg-config
```

On Fedora:

```bash
sudo dnf install gtk4-devel libadwaita-devel gcc pkgconf-pkg-config
```

On Arch:

```bash
sudo pacman -S gtk4 libadwaita rust mihomo
```

## Build

```bash
cd Clash-GNOME
cargo build --release
```

Binary at `target/release/clash-gnome`.

## Install

```bash
sudo install -Dm755 target/release/clash-gnome /usr/local/bin/clash-gnome
sudo install -Dm644 data/io.langya.ClashGNOME.desktop /usr/share/applications/io.langya.ClashGNOME.desktop
sudo install -Dm644 data/io.langya.ClashGNOME.metainfo.xml /usr/share/metainfo/io.langya.ClashGNOME.metainfo.xml
```

## First run

1. Open Clash GNOME.
2. Go to **Settings** and set the path to your `mihomo` binary (auto-detected under `/usr/bin/mihomo`).
3. Go to **Subscriptions**, click **Add Subscription**:
   - **From URL** — paste your Clash/mihomo YAML subscription URL.
   - **From local file** — pick a local `.yaml` / `.yml` config.
4. Back to **Dashboard**, click **Start Core**.
5. Configure your system proxy to `http://127.0.0.1:7890` (mixed port), or enable **TUN mode** for transparent capture.

### Enabling TUN mode

TUN requires the mihomo binary to have `cap_net_admin`. From **Settings** -> **Grant TUN capabilities**, click **Run setcap** — this launches `pkexec` for a one-time authorization. Equivalent shell command:

```bash
sudo setcap cap_net_admin,cap_net_bind_service,cap_dac_override,cap_sys_ptrace+eip /usr/bin/mihomo
```

## Storage layout

- App config: `~/.config/clash-gnome/config.json`
- Subscriptions: `~/.local/share/clash-gnome/subscriptions/<id>.yaml`
- Generated mihomo config: `~/.local/share/clash-gnome/mihomo/config.yaml`
- Autostart: `~/.config/autostart/io.langya.ClashGNOME.desktop`

Every setting change (mode, ports, TUN toggle, theme, subscriptions, etc.) is written to `config.json` immediately, so all state survives an app restart.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
