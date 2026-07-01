# CLAUDE.md — Clash-GNOME project notes

Instructions for Claude Code when working on this repo. Kept short and factual.

## Project shape

- Native GTK4 + libadwaita desktop app for the mihomo (Clash Meta) proxy core.
- Rust, edition 2024. GNOME HIG. No web wrapping. No Electron.
- Communicates with mihomo via its RESTful external-controller API (JSON + line-delimited streams).
- Config lives at `~/.config/clash-gnome/config.json`; subscription YAMLs at `~/.local/share/clash-gnome/subscriptions/`.
- App ID is `io.langya.ClashGNOME` (not `io.github.langya.*`).

## Coding rules (enforce silently)

- No emojis anywhere: chat, code, README, commit messages, PR descriptions.
- Prefer editing existing files over creating new ones.
- Don't add speculative abstractions or backwards-compat shims.
- Only add comments for non-obvious *why*. Never restate what the code does.
- Rust: strict clippy — the repo compiles with zero warnings. Keep it that way.

## Building locally

Debian / Ubuntu / Zorin:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libdbus-1-dev libssl-dev build-essential pkg-config
cargo build --release
./target/release/clash-gnome
```

Debug launch: `./target/release/clash-gnome`. Wait 3-5 seconds — window shows even without mihomo installed.

Verify no warnings before commit:

```bash
cargo build --release 2>&1 | grep -E "warning:|error"      # expect empty
cargo clippy --release --quiet 2>&1 | tail -5              # expect empty
```

## Releasing

Releases are cut from git tags. The GitHub Actions workflow at `.github/workflows/release.yml` builds `x86_64` and `aarch64` on push of a `v*.*.*` tag, packages a `.tar.gz` and a `.deb` for each arch, then publishes a GitHub Release with all four artifacts.

**Steps:**

1. Bump `version` in `Cargo.toml` (and any hard-coded version in `data/*.metainfo.xml` `<releases>` block).
2. Update `data/io.langya.ClashGNOME.metainfo.xml` with a new `<release>` entry.
3. Commit: `git commit -am "Release vX.Y.Z"`.
4. Tag: `git tag vX.Y.Z && git push origin main --tags`.
5. Watch the run: `gh run watch` (workflow name: **Release**).
6. When complete, check <https://github.com/LangYa466/Clash-GNOME/releases> — the release is auto-populated with generated release notes and the four artifacts:
   - `clash-gnome-vX.Y.Z-x86_64-linux.tar.gz`
   - `clash-gnome-vX.Y.Z-aarch64-linux.tar.gz`
   - `clash-gnome_X.Y.Z-1_amd64.deb`
   - `clash-gnome_X.Y.Z-1_arm64.deb`

**Manual dispatch:** the workflow can also be triggered without tagging via `gh workflow run release.yml -f tag=vX.Y.Z-test` — that produces the artifacts as workflow outputs but does not publish a Release (the `release` job only runs on tag pushes).

## Architecture map

```
src/main.rs                     entry, inits tokio runtime and env_logger
src/application.rs              AdwApplication subclass, global actions, auto-update ticker, tray install, app.hold()
src/window.rs                   AdwApplicationWindow + AdwNavigationSplitView + view stack
src/config.rs                   SharedConfig (Arc<RwLock<AppConfig>>), JSON persistence
src/api.rs                      mihomo REST client + streaming endpoints (/traffic, /memory, /logs)
src/core_manager.rs             spawn/stop mihomo, wait-for-ready, log pipe, setcap-via-pkexec
src/mihomo_config.rs            merge subscription YAML + user settings -> config.yaml
src/subscription.rs             HTTP fetch (with optional proxy or bypass), file:// support, quota parsing
src/autostart.rs                XDG autostart .desktop management
src/tray.rs                     ksni StatusNotifierItem (Show/Hide, Start/Stop, Mode, TUN, Quit)
src/util.rs                     tokio<->GLib bridge, byte/speed formatting, system proxy summary
src/pages/{dashboard,proxies,rules,connections,subscriptions,logs,settings}.rs
data/style.css                  compiled into the binary via include_str!
data/io.langya.ClashGNOME.*     desktop, metainfo, gschema
```

Threading model: one tokio multi-thread runtime (2 workers) for network / process I/O; results marshalled to the GLib main loop via `async_channel` in `util::spawn`. Never call `.await` on the main thread; never touch GTK widgets from tokio tasks.

## Common pitfalls

- **Pango markup in Preferences titles**: `&`, `<`, `>` in `AdwPreferencesGroup::set_title` are markup. Use `&amp;` etc. Example: "Core &amp; Mode".
- **RefCell borrow in `if let`**: `if let Some(x) = self.foo.borrow().clone() { ... } else { self.foo.borrow_mut() = ... }` panics — the Ref from the head expression lives into the else branch. Hoist: `let cur = self.foo.borrow().clone();` first.
- **CssProvider::load_from_string** requires v4_12 feature. On v4_10, use `load_from_data(include_str!(...))`.
- **ListView factory rebind stale handler**: connecting a signal on every `connect_bind` accumulates handlers on recycled widgets. Connect ONCE in `connect_setup` and read the current item via `item_weak.upgrade().and_then(|i| i.item().and_downcast::<T>())`.
- **App lifetime and tray**: when the window closes the app would normally quit. `self.obj().hold()` returns a guard — `std::mem::forget(guard)` keeps the app alive until an explicit `app.quit()` action.
- **DB / schema not applicable here** but Rust equivalents: never silently default to a hardcoded mihomo path; fail loud in `default_mihomo_path` if nothing is found (currently returns `"mihomo"` for PATH lookup — acceptable).

## Testing UI changes

After any UI change, actually launch and screenshot:

```bash
./target/release/clash-gnome > /tmp/run.log 2>&1 &
PID=$!
sleep 3
xwd -id $(xwininfo -root -tree -display :1 | grep '"Clash GNOME": ("clash-gnome"' | awk '{print $1}' | head -1) -display :1 -silent > /tmp/shot.xwd
convert /tmp/shot.xwd /tmp/shot.png
kill $PID
```

If `/tmp/run.log` is non-empty after 3s of running, something warned or panicked — fix before moving on.

## Do not

- Do not touch `~/.config/clash-gnome/config.json` on behalf of the user for testing; delete it if you need a fresh state, but note that the user has a real subscription stored there.
