use crate::application::AppState;
use crate::core_manager::CoreState;
use gtk::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum TrayEvent {
    ToggleWindow,
    StartCore,
    StopCore,
    SetMode(String),
    ToggleTun,
    Quit,
}

#[derive(Default)]
struct TrayState {
    core_running: bool,
    mode: String,
    tun: bool,
}

pub struct ClashTray {
    tx: async_channel::Sender<TrayEvent>,
    inner: Arc<Mutex<TrayState>>,
}

impl ksni::Tray for ClashTray {
    fn icon_name(&self) -> String {
        let s = self.inner.lock().unwrap();
        if s.core_running { "network-vpn-symbolic".into() } else { "network-vpn-disabled-symbolic".into() }
    }
    fn title(&self) -> String { "Clash GNOME".into() }
    fn id(&self) -> String { "io.langya.ClashGNOME".into() }
    fn tool_tip(&self) -> ksni::ToolTip {
        let s = self.inner.lock().unwrap();
        ksni::ToolTip {
            title: "Clash GNOME".into(),
            description: format!(
                "Core: {} | Mode: {} | TUN: {}",
                if s.core_running { "Running" } else { "Stopped" },
                title_case(&s.mode),
                if s.tun { "On" } else { "Off" },
            ),
            icon_name: self.icon_name(),
            icon_pixmap: vec![],
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(TrayEvent::ToggleWindow);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let s = self.inner.lock().unwrap();
        let running = s.core_running;
        let cur_mode = s.mode.clone();
        let tun_on = s.tun;
        drop(s);
        let is_mode = move |m: &str| cur_mode == m;
        let cur_mode_disp = title_case(&self.inner.lock().unwrap().mode);
        vec![
            StandardItem {
                label: "Show / Hide window".into(),
                icon_name: "view-reveal-symbolic".into(),
                activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::ToggleWindow); }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: if running { "Stop Core".into() } else { "Start Core".into() },
                icon_name: if running { "media-playback-stop-symbolic".into() } else { "media-playback-start-symbolic".into() },
                activate: Box::new(move |t: &mut ClashTray| {
                    let ev = if t.inner.lock().unwrap().core_running { TrayEvent::StopCore } else { TrayEvent::StartCore };
                    let _ = t.tx.try_send(ev);
                }),
                ..Default::default()
            }.into(),
            SubMenu {
                label: format!("Mode: {}", cur_mode_disp),
                submenu: vec![
                    CheckmarkItem {
                        label: "Rule".into(),
                        checked: is_mode("rule"),
                        activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::SetMode("rule".into())); }),
                        ..Default::default()
                    }.into(),
                    CheckmarkItem {
                        label: "Global".into(),
                        checked: is_mode("global"),
                        activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::SetMode("global".into())); }),
                        ..Default::default()
                    }.into(),
                    CheckmarkItem {
                        label: "Direct".into(),
                        checked: is_mode("direct"),
                        activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::SetMode("direct".into())); }),
                        ..Default::default()
                    }.into(),
                ],
                ..Default::default()
            }.into(),
            CheckmarkItem {
                label: "TUN Mode".into(),
                checked: tun_on,
                activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::ToggleTun); }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|t: &mut ClashTray| { let _ = t.tx.try_send(TrayEvent::Quit); }),
                ..Default::default()
            }.into(),
        ]
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub struct TrayCtl {
    handle: ksni::Handle<ClashTray>,
    inner: Arc<Mutex<TrayState>>,
}

impl TrayCtl {
    fn refresh(&self) {
        self.handle.update(|_| {});
    }
    pub fn set_running(&self, running: bool) {
        let changed = {
            let mut g = self.inner.lock().unwrap();
            let c = g.core_running != running;
            g.core_running = running;
            c
        };
        if changed { self.refresh(); }
    }
    pub fn set_mode(&self, mode: &str) {
        let changed = {
            let mut g = self.inner.lock().unwrap();
            let c = g.mode != mode;
            if c { g.mode = mode.to_string(); }
            c
        };
        if changed { self.refresh(); }
    }
    pub fn set_tun(&self, on: bool) {
        let changed = {
            let mut g = self.inner.lock().unwrap();
            let c = g.tun != on;
            g.tun = on;
            c
        };
        if changed { self.refresh(); }
    }
}

pub fn install(state: Arc<AppState>) -> Option<Arc<TrayCtl>> {
    let (tx, rx) = async_channel::unbounded::<TrayEvent>();
    let (initial_mode, initial_tun) = {
        let cfg = state.cfg.read().unwrap();
        (cfg.mode.clone(), cfg.tun_enabled)
    };
    let inner = Arc::new(Mutex::new(TrayState {
        core_running: false,
        mode: initial_mode,
        tun: initial_tun,
    }));
    let tray = ClashTray { tx: tx.clone(), inner: inner.clone() };
    let service = ksni::TrayService::new(tray);
    let handle = service.handle();
    service.spawn();
    let ctl = Arc::new(TrayCtl { handle, inner });

    // Consume events on the GLib main loop.
    let state_c = state.clone();
    let ctl_c = ctl.clone();
    gtk::glib::MainContext::default().spawn_local(async move {
        while let Ok(ev) = rx.recv().await {
            handle_event(&state_c, &ctl_c, ev).await;
        }
    });

    // Poll core state to keep tray icon accurate.
    let state_c = state.clone();
    let ctl_c = ctl.clone();
    gtk::glib::timeout_add_seconds_local(1, move || {
        let core = state_c.core.clone();
        let ctl_c = ctl_c.clone();
        crate::util::spawn(async move { core.state().await }, move |s| {
            ctl_c.set_running(matches!(s, CoreState::Running | CoreState::Starting));
        });
        gtk::glib::ControlFlow::Continue
    });

    // Poll config state (mode / TUN) so external changes reflect in tray.
    let state_c = state.clone();
    let ctl_c = ctl.clone();
    gtk::glib::timeout_add_seconds_local(1, move || {
        let cfg = state_c.cfg.read().unwrap();
        ctl_c.set_mode(&cfg.mode);
        ctl_c.set_tun(cfg.tun_enabled);
        gtk::glib::ControlFlow::Continue
    });

    Some(ctl)
}

async fn handle_event(state: &Arc<AppState>, ctl: &Arc<TrayCtl>, ev: TrayEvent) {
    match ev {
        TrayEvent::ToggleWindow => {
            let app = match gtk::gio::Application::default().and_then(|a| a.downcast::<gtk::Application>().ok()) {
                Some(a) => a,
                None => return,
            };
            if let Some(w) = app.active_window() {
                if w.is_visible() && w.is_active() {
                    w.set_visible(false);
                } else {
                    w.set_visible(true);
                    w.present();
                }
            } else {
                app.activate();
            }
        }
        TrayEvent::StartCore => {
            let core = state.core.clone();
            crate::util::detach(async move { let _ = core.start().await; });
        }
        TrayEvent::StopCore => {
            let core = state.core.clone();
            crate::util::detach(async move { let _ = core.stop().await; });
        }
        TrayEvent::SetMode(m) => {
            state.cfg.write().unwrap().mode = m.clone();
            let _ = crate::config::persist(&state.cfg);
            ctl.set_mode(&m);
            let core = state.core.clone();
            let m2 = m.clone();
            crate::util::detach(async move {
                let api = core.api();
                let _ = api.patch_configs(serde_json::json!({ "mode": m2 })).await;
            });
        }
        TrayEvent::ToggleTun => {
            let new_val = {
                let mut g = state.cfg.write().unwrap();
                g.tun_enabled = !g.tun_enabled;
                g.tun_enabled
            };
            let _ = crate::config::persist(&state.cfg);
            ctl.set_tun(new_val);
            let core = state.core.clone();
            crate::util::detach(async move { let _ = core.apply_config().await; });
        }
        TrayEvent::Quit => {
            if let Some(app) = gtk::gio::Application::default() {
                app.activate_action("quit", None);
            }
        }
    }
}
