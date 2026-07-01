use crate::application::AppState;
use crate::config::ThemeMode;
use crate::{autostart, core_manager, util};
use adw::prelude::*;
use std::sync::Arc;

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let page = adw::PreferencesPage::new();
    page.set_title("Settings");
    page.set_icon_name(Some("preferences-system-symbolic"));

    // === Appearance ===
    let appearance = adw::PreferencesGroup::new();
    appearance.set_title("Appearance");
    appearance.set_description(Some("Adaptive light/dark styling"));

    let theme_row = adw::ComboRow::new();
    theme_row.set_title("Theme");
    theme_row.set_subtitle("Follows system by default");
    let theme_model = gtk::StringList::new(&["System", "Light", "Dark"]);
    theme_row.set_model(Some(&theme_model));
    theme_row.set_selected(match state.cfg.read().unwrap().theme {
        ThemeMode::System => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    });
    {
        let state = state.clone();
        theme_row.connect_selected_notify(move |row| {
            let mode = match row.selected() {
                1 => ThemeMode::Light,
                2 => ThemeMode::Dark,
                _ => ThemeMode::System,
            };
            state.cfg.write().unwrap().theme = mode;
            let _ = crate::config::persist(&state.cfg);
            crate::application::apply_theme(mode);
        });
    }
    appearance.add(&theme_row);
    page.add(&appearance);

    // === Startup ===
    let startup = adw::PreferencesGroup::new();
    startup.set_title("Startup");

    let autostart_row = adw::SwitchRow::new();
    autostart_row.set_title("Launch at login");
    autostart_row.set_subtitle("Adds an XDG autostart entry (hidden window)");
    autostart_row.set_active(autostart::is_enabled());
    {
        let state = state.clone();
        autostart_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            if let Err(e) = autostart::set(enabled) {
                log::warn!("autostart set failed: {e}");
            }
            state.cfg.write().unwrap().autostart = enabled;
            let _ = crate::config::persist(&state.cfg);
        });
    }
    startup.add(&autostart_row);

    let start_core_row = adw::SwitchRow::new();
    start_core_row.set_title("Start mihomo core on launch");
    start_core_row.set_subtitle("Automatically bring the proxy up when this app opens");
    start_core_row.set_active(state.cfg.read().unwrap().start_kernel_on_launch);
    {
        let state = state.clone();
        start_core_row.connect_active_notify(move |row| {
            state.cfg.write().unwrap().start_kernel_on_launch = row.is_active();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    startup.add(&start_core_row);
    page.add(&startup);

    // === Core ===
    let core_g = adw::PreferencesGroup::new();
    core_g.set_title("Kernel");

    let path_row = adw::EntryRow::new();
    path_row.set_title("mihomo binary path");
    path_row.set_text(&state.cfg.read().unwrap().mihomo_path);
    {
        let state = state.clone();
        path_row.connect_changed(move |row| {
            state.cfg.write().unwrap().mihomo_path = row.text().to_string();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    core_g.add(&path_row);

    let browse_btn = gtk::Button::from_icon_name("document-open-symbolic");
    browse_btn.add_css_class("flat");
    browse_btn.set_valign(gtk::Align::Center);
    path_row.add_suffix(&browse_btn);
    {
        let state = state.clone();
        let path_row = path_row.clone();
        browse_btn.connect_clicked(move |btn| {
            let dialog = gtk::FileDialog::builder()
                .title("Choose mihomo binary")
                .modal(true)
                .build();
            let path_row = path_row.clone();
            let state = state.clone();
            let win = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            dialog.open(win.as_ref(), None::<&gio::Cancellable>, move |res| {
                if let Ok(file) = res
                    && let Some(p) = file.path() {
                        let s = p.to_string_lossy().into_owned();
                        path_row.set_text(&s);
                        state.cfg.write().unwrap().mihomo_path = s;
                        let _ = crate::config::persist(&state.cfg);
                    }
            });
        });
    }

    let setcap_row = adw::ActionRow::new();
    setcap_row.set_title("Grant TUN capabilities");
    setcap_row.set_subtitle("Sets cap_net_admin on the mihomo binary via pkexec. Required for TUN mode.");
    let setcap_btn = gtk::Button::with_label("Run setcap…");
    setcap_btn.set_valign(gtk::Align::Center);
    setcap_btn.add_css_class("pill");
    setcap_row.add_suffix(&setcap_btn);
    {
        let state = state.clone();
        setcap_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let binary = state.cfg.read().unwrap().mihomo_path.clone();
            let b_c = b.clone();
            let win = b.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            util::spawn(async move {
                core_manager::setcap_via_pkexec(&binary).await
            }, move |res| {
                b_c.set_sensitive(true);
                let dlg = adw::AlertDialog::builder()
                    .heading(if res.is_ok() { "Capabilities granted" } else { "setcap failed" })
                    .body(match &res { Ok(_) => "You can now enable TUN mode.".to_string(), Err(e) => e.to_string() })
                    .build();
                dlg.add_response("ok", "OK");
                dlg.present(win.as_ref());
            });
        });
    }
    core_g.add(&setcap_row);

    page.add(&core_g);

    // === Ports ===
    let ports = adw::PreferencesGroup::new();
    ports.set_title("Local Ports");

    add_port_row(&ports, "Mixed port", "HTTP + SOCKS combined", state.clone(), |cfg, v| cfg.mixed_port = v, |cfg| cfg.mixed_port);
    add_port_row(&ports, "SOCKS port", "SOCKS5 only", state.clone(), |cfg, v| cfg.socks_port = v, |cfg| cfg.socks_port);
    add_port_row(&ports, "HTTP port", "HTTP only", state.clone(), |cfg, v| cfg.http_port = v, |cfg| cfg.http_port);
    add_port_row(&ports, "External controller port", "RESTful API port", state.clone(), |cfg, v| cfg.api_port = v, |cfg| cfg.api_port);

    let host_row = adw::EntryRow::new();
    host_row.set_title("External controller host");
    host_row.set_text(&state.cfg.read().unwrap().api_host);
    {
        let state = state.clone();
        host_row.connect_changed(move |row| {
            state.cfg.write().unwrap().api_host = row.text().to_string();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    ports.add(&host_row);

    let secret_row = adw::PasswordEntryRow::new();
    secret_row.set_title("API secret (optional)");
    secret_row.set_text(&state.cfg.read().unwrap().api_secret);
    {
        let state = state.clone();
        secret_row.connect_changed(move |row| {
            state.cfg.write().unwrap().api_secret = row.text().to_string();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    ports.add(&secret_row);

    let ipv6_row = adw::SwitchRow::new();
    ipv6_row.set_title("Enable IPv6");
    ipv6_row.set_subtitle("Route IPv6 traffic through the proxy");
    ipv6_row.set_active(state.cfg.read().unwrap().ipv6);
    {
        let state = state.clone();
        ipv6_row.connect_active_notify(move |row| {
            state.cfg.write().unwrap().ipv6 = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            let v = row.is_active();
            util::detach(async move {
                let _ = core.apply_config().await;
                let api = core.api();
                let _ = api.patch_configs(serde_json::json!({ "ipv6": v })).await;
            });
        });
    }
    ports.add(&ipv6_row);

    let lan_row = adw::SwitchRow::new();
    lan_row.set_title("Allow LAN");
    lan_row.set_subtitle("Accept connections from other devices on the network");
    lan_row.set_active(state.cfg.read().unwrap().allow_lan);
    {
        let state = state.clone();
        lan_row.connect_active_notify(move |row| {
            state.cfg.write().unwrap().allow_lan = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    ports.add(&lan_row);
    page.add(&ports);

    // === TUN ===
    let tun = adw::PreferencesGroup::new();
    tun.set_title("TUN Mode");
    tun.set_description(Some("Transparent proxy at the network layer"));

    let tun_enable = adw::SwitchRow::new();
    tun_enable.set_title("Enable TUN");
    tun_enable.set_active(state.cfg.read().unwrap().tun_enabled);
    {
        let state = state.clone();
        tun_enable.connect_active_notify(move |row| {
            state.cfg.write().unwrap().tun_enabled = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    tun.add(&tun_enable);

    let stack_row = adw::ComboRow::new();
    stack_row.set_title("TUN stack");
    let stack_model = gtk::StringList::new(&["system", "gvisor", "mixed"]);
    stack_row.set_model(Some(&stack_model));
    stack_row.set_selected(match state.cfg.read().unwrap().tun_stack.as_str() {
        "gvisor" => 1,
        "mixed" => 2,
        _ => 0,
    });
    {
        let state = state.clone();
        stack_row.connect_selected_notify(move |row| {
            let s = match row.selected() {
                1 => "gvisor",
                2 => "mixed",
                _ => "system",
            };
            state.cfg.write().unwrap().tun_stack = s.to_string();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    tun.add(&stack_row);

    let auto_route = adw::SwitchRow::new();
    auto_route.set_title("Auto route");
    auto_route.set_subtitle("Automatically configure system routing to send traffic through the TUN device");
    auto_route.set_active(state.cfg.read().unwrap().tun_auto_route);
    {
        let state = state.clone();
        auto_route.connect_active_notify(move |row| {
            state.cfg.write().unwrap().tun_auto_route = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    tun.add(&auto_route);

    let auto_detect = adw::SwitchRow::new();
    auto_detect.set_title("Auto-detect interface");
    auto_detect.set_active(state.cfg.read().unwrap().tun_auto_detect_interface);
    {
        let state = state.clone();
        auto_detect.connect_active_notify(move |row| {
            state.cfg.write().unwrap().tun_auto_detect_interface = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    tun.add(&auto_detect);

    page.add(&tun);

    // === DNS ===
    let dns = adw::PreferencesGroup::new();
    dns.set_title("DNS");

    let dns_enable = adw::SwitchRow::new();
    dns_enable.set_title("Enable internal DNS");
    dns_enable.set_active(state.cfg.read().unwrap().dns_enable);
    {
        let state = state.clone();
        dns_enable.connect_active_notify(move |row| {
            state.cfg.write().unwrap().dns_enable = row.is_active();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    dns.add(&dns_enable);

    let enhanced_row = adw::ComboRow::new();
    enhanced_row.set_title("Enhanced mode");
    let enhanced_model = gtk::StringList::new(&["fake-ip", "redir-host", "normal"]);
    enhanced_row.set_model(Some(&enhanced_model));
    enhanced_row.set_selected(match state.cfg.read().unwrap().dns_enhanced_mode.as_str() {
        "redir-host" => 1,
        "normal" => 2,
        _ => 0,
    });
    {
        let state = state.clone();
        enhanced_row.connect_selected_notify(move |row| {
            let m = match row.selected() {
                1 => "redir-host",
                2 => "normal",
                _ => "fake-ip",
            };
            state.cfg.write().unwrap().dns_enhanced_mode = m.to_string();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    dns.add(&enhanced_row);

    let listen_row = adw::EntryRow::new();
    listen_row.set_title("DNS listen");
    listen_row.set_text(&state.cfg.read().unwrap().dns_listen);
    {
        let state = state.clone();
        listen_row.connect_changed(move |row| {
            state.cfg.write().unwrap().dns_listen = row.text().to_string();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    dns.add(&listen_row);
    page.add(&dns);

    // === Log level ===
    let logs = adw::PreferencesGroup::new();
    logs.set_title("Logging");
    let level_row = adw::ComboRow::new();
    level_row.set_title("Log level");
    let level_model = gtk::StringList::new(&["silent", "error", "warning", "info", "debug"]);
    level_row.set_model(Some(&level_model));
    level_row.set_selected(match state.cfg.read().unwrap().log_level.as_str() {
        "silent" => 0,
        "error" => 1,
        "warning" => 2,
        "debug" => 4,
        _ => 3,
    });
    {
        let state = state.clone();
        level_row.connect_selected_notify(move |row| {
            let l = ["silent", "error", "warning", "info", "debug"][row.selected() as usize];
            state.cfg.write().unwrap().log_level = l.to_string();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            let ll = l.to_string();
            util::detach(async move {
                let api = core.api();
                let _ = api.patch_configs(serde_json::json!({ "log-level": ll })).await;
            });
        });
    }
    logs.add(&level_row);

    let size_row = adw::SpinRow::with_range(1.0, 4096.0, 1.0);
    size_row.set_title("Log file max size (MB)");
    size_row.set_value(state.cfg.read().unwrap().log_max_size_mb as f64);
    {
        let state = state.clone();
        size_row.connect_value_notify(move |r| {
            state.cfg.write().unwrap().log_max_size_mb = r.value() as u32;
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    logs.add(&size_row);

    let days_row = adw::SpinRow::with_range(1.0, 365.0, 1.0);
    days_row.set_title("Log retention (days)");
    days_row.set_value(state.cfg.read().unwrap().log_max_days as f64);
    {
        let state = state.clone();
        days_row.connect_value_notify(move |r| {
            state.cfg.write().unwrap().log_max_days = r.value() as u32;
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move { let _ = core.apply_config().await; });
        });
    }
    logs.add(&days_row);
    page.add(&logs);

    // === Subscription ===
    let sub_g = adw::PreferencesGroup::new();
    sub_g.set_title("Subscription");

    let ua_row = adw::EntryRow::new();
    ua_row.set_title("User-Agent");
    ua_row.set_text(&state.cfg.read().unwrap().subscription_user_agent);
    let ua_reset = gtk::Button::from_icon_name("edit-undo-symbolic");
    ua_reset.add_css_class("flat");
    ua_reset.set_valign(gtk::Align::Center);
    ua_reset.set_tooltip_text(Some("Reset to default"));
    ua_row.add_suffix(&ua_reset);
    {
        let state = state.clone();
        ua_row.connect_changed(move |row| {
            state.cfg.write().unwrap().subscription_user_agent = row.text().to_string();
            let _ = crate::config::persist(&state.cfg);
        });
    }
    {
        let state = state.clone();
        let ua_row = ua_row.clone();
        ua_reset.connect_clicked(move |_| {
            let d = crate::config::default_user_agent();
            ua_row.set_text(&d);
            state.cfg.write().unwrap().subscription_user_agent = d;
            let _ = crate::config::persist(&state.cfg);
        });
    }
    sub_g.add(&ua_row);
    page.add(&sub_g);

    // === Danger zone ===
    let danger = adw::PreferencesGroup::new();
    danger.set_title("Actions");
    let restart_row = adw::ActionRow::new();
    restart_row.set_title("Restart core");
    let restart_btn = gtk::Button::with_label("Restart");
    restart_btn.set_valign(gtk::Align::Center);
    restart_btn.add_css_class("pill");
    restart_row.add_suffix(&restart_btn);
    {
        let state = state.clone();
        restart_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let core = state.core.clone();
            let b_c = b.clone();
            util::spawn(async move { core.restart().await }, move |_| { b_c.set_sensitive(true); });
        });
    }
    danger.add(&restart_row);
    page.add(&danger);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&page)
        .build();
    scrolled.upcast()
}

use gtk::gio;

fn add_port_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: &str,
    state: Arc<AppState>,
    set: impl Fn(&mut crate::config::AppConfig, u16) + 'static,
    get: impl Fn(&crate::config::AppConfig) -> u16 + 'static,
) {
    let row = adw::SpinRow::with_range(1.0, 65535.0, 1.0);
    row.set_title(title);
    row.set_subtitle(subtitle);
    let cur = get(&state.cfg.read().unwrap()) as f64;
    row.set_value(cur);
    let state_c = state.clone();
    row.connect_value_notify(move |r| {
        let v = r.value() as u16;
        {
            let mut cfg = state_c.cfg.write().unwrap();
            set(&mut cfg, v);
        }
        let _ = crate::config::persist(&state_c.cfg);
        let core = state_c.core.clone();
        util::detach(async move { let _ = core.apply_config().await; });
    });
    group.add(&row);
}
