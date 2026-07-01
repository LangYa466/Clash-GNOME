use crate::config::{self, SharedConfig, ThemeMode, APP_ID};
use crate::core_manager::CoreManager;
use crate::window::MainWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio;
use gtk::glib;
use std::cell::RefCell;
use std::sync::Arc;

pub struct AppState {
    pub cfg: SharedConfig,
    pub core: Arc<CoreManager>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let cfg = config::shared();
        let core = CoreManager::new(cfg.clone());
        Arc::new(AppState { cfg, core })
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ClashGnomeApp {
        pub state: RefCell<Option<Arc<AppState>>>,
        pub window: RefCell<Option<MainWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClashGnomeApp {
        const NAME: &'static str = "ClashGnomeApp";
        type Type = super::ClashGnomeApp;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for ClashGnomeApp {}

    impl ApplicationImpl for ClashGnomeApp {
        fn startup(&self) {
            self.parent_startup();
            let _ = config::ensure_dirs();
            let state = AppState::new();
            *self.state.borrow_mut() = Some(state.clone());
            load_css();
            apply_theme(state.cfg.read().unwrap().theme);
            setup_actions(&self.obj(), &state);
        }

        fn activate(&self) {
            let app = self.obj();
            let state = self.state.borrow().clone().expect("state initialized in startup");

            let hidden = std::env::args().any(|a| a == "--hidden");

            let existing = self.window.borrow().clone();
            let window = if let Some(w) = existing {
                w
            } else {
                let w = MainWindow::new(&app, state.clone());
                *self.window.borrow_mut() = Some(w.clone());
                w
            };

            if !hidden {
                window.present();
            }

            // Optionally auto-start kernel on launch
            let should_start = state.cfg.read().unwrap().start_kernel_on_launch;
            if should_start {
                let core = state.core.clone();
                crate::util::detach(async move {
                    if let Err(e) = core.start().await {
                        log::warn!("auto-start kernel failed: {e}");
                    }
                });
            }

            // Subscription auto-update ticker (one per app lifetime).
            static AUTO_UPDATE_STARTED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !AUTO_UPDATE_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                start_auto_update_ticker(state.clone());
                let _ = crate::tray::install(state.clone());
                // Prevent the app from exiting when the window closes so the tray keeps running.
                let guard = self.obj().hold();
                std::mem::forget(guard);
            }
        }
    }

    impl GtkApplicationImpl for ClashGnomeApp {}
    impl AdwApplicationImpl for ClashGnomeApp {}
}

glib::wrapper! {
    pub struct ClashGnomeApp(ObjectSubclass<imp::ClashGnomeApp>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ClashGnomeApp {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("flags", gio::ApplicationFlags::default())
            .property("resource-base-path", "/io/github/langya/ClashGNOME")
            .build()
    }
}

impl Default for ClashGnomeApp {
    fn default() -> Self { Self::new() }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../data/style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn apply_theme(mode: ThemeMode) {
    let mgr = adw::StyleManager::default();
    let scheme = match mode {
        ThemeMode::System => adw::ColorScheme::Default,
        ThemeMode::Light => adw::ColorScheme::ForceLight,
        ThemeMode::Dark => adw::ColorScheme::ForceDark,
    };
    mgr.set_color_scheme(scheme);
}

fn start_auto_update_ticker(state: Arc<AppState>) {
    glib::timeout_add_seconds_local(60, move || {
        let now = chrono::Utc::now();
        let (due, ua): (Vec<crate::config::Subscription>, String) = {
            let cfg = state.cfg.read().unwrap();
            let subs = cfg.subscriptions.iter()
                .filter(|s| s.auto_update)
                .filter(|s| !s.url.starts_with("file://"))
                .filter(|s| {
                    let last = s.updated_at;
                    let interval_min = s.auto_update_unit.to_minutes(s.auto_update_value.max(1)) as i64;
                    match last {
                        None => true,
                        Some(t) => (now - t).num_minutes() >= interval_min,
                    }
                })
                .cloned()
                .collect();
            (subs, cfg.subscription_user_agent.clone())
        };
        for sub in due {
            let proxy_url = if sub.use_proxy_for_update {
                let cfg = state.cfg.read().unwrap();
                Some(format!("http://{}:{}", cfg.api_host, cfg.mixed_port))
            } else { None };
            let state = state.clone();
            let sub_id = sub.id.clone();
            let ua = ua.clone();
            crate::util::spawn(async move {
                let out = crate::subscription::fetch_with_proxy(&sub, &ua, proxy_url.as_deref()).await;
                (sub_id, out)
            }, move |(sub_id, out)| {
                match out {
                    Ok(o) => {
                        let is_active;
                        {
                            let mut g = state.cfg.write().unwrap();
                            if let Some(s) = g.subscriptions.iter_mut().find(|s| s.id == sub_id) {
                                s.upload = o.upload;
                                s.download = o.download;
                                s.total = o.total;
                                s.expire = o.expire;
                                s.updated_at = Some(chrono::Utc::now());
                            }
                            is_active = g.active_subscription.as_deref() == Some(&sub_id);
                        }
                        let _ = crate::config::persist(&state.cfg);
                        if is_active {
                            let core = state.core.clone();
                            crate::util::detach(async move { let _ = core.apply_config().await; });
                        }
                        log::info!("Auto-updated subscription {sub_id}");
                    }
                    Err(e) => log::warn!("Auto-update {sub_id} failed: {e}"),
                }
            });
        }
        glib::ControlFlow::Continue
    });
}

fn setup_actions(app: &ClashGnomeApp, state: &Arc<AppState>) {
    let quit = gio::SimpleAction::new("quit", None);
    let core = state.core.clone();
    let app_weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        let core = core.clone();
        let app_weak = app_weak.clone();
        crate::util::spawn(
            async move {
                let _ = core.stop().await;
            },
            move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.quit();
                }
            },
        );
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Ctrl>q"]);

    let about = gio::SimpleAction::new("about", None);
    let app_weak = app.downgrade();
    about.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            let win = app.active_window();
            let dialog = adw::AboutDialog::builder()
                .application_name("Clash GNOME")
                .application_icon("network-vpn-symbolic")
                .developer_name("LangYa466")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/LangYa466/Clash-GNOME")
                .issue_url("https://github.com/LangYa466/Clash-GNOME/issues")
                .license_type(gtk::License::Gpl30)
                .comments("A modern GTK4/libadwaita GUI for mihomo (Clash Meta)")
                .build();
            dialog.present(win.as_ref());
        }
    });
    app.add_action(&about);
}
