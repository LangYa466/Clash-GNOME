use crate::application::AppState;
use crate::core_manager::CoreState;
use crate::util;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let clamp = adw::Clamp::builder()
        .maximum_size(1100)
        .tightening_threshold(900)
        .child(&content(state))
        .build();

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .build();
    scrolled.upcast()
}

fn content(state: Arc<AppState>) -> gtk::Box {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 18);
    vbox.set_margin_top(24);
    vbox.set_margin_bottom(24);
    vbox.set_margin_start(24);
    vbox.set_margin_end(24);

    // Title
    let title = gtk::Label::new(Some("Overview"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    vbox.append(&title);

    // Stat cards row
    let cards = gtk::FlowBox::builder()
        .homogeneous(true)
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(2)
        .max_children_per_line(4)
        .row_spacing(14)
        .column_spacing(14)
        .build();

    let up_card = stat_card("Upload", "0 B/s", "go-up-symbolic", "accent-blue");
    let down_card = stat_card("Download", "0 B/s", "go-down-symbolic", "accent-purple");
    let up_total_card = stat_card("Session Up", "0 B", "network-transmit-symbolic", "accent-teal");
    let down_total_card = stat_card("Session Down", "0 B", "network-receive-symbolic", "accent-green");
    let mem_card = stat_card("Memory", "-", "drive-harddisk-symbolic", "accent-orange");
    let conn_card = stat_card("Connections", "0", "network-workgroup-symbolic", "accent-pink");

    let up_value = up_card.value_label.clone();
    let down_value = down_card.value_label.clone();
    let up_total_value = up_total_card.value_label.clone();
    let down_total_value = down_total_card.value_label.clone();
    let mem_value = mem_card.value_label.clone();
    let conn_value = conn_card.value_label.clone();

    for c in [&up_card.root, &down_card.root, &up_total_card.root, &down_total_card.root, &mem_card.root, &conn_card.root] {
        cards.insert(c, -1);
    }
    vbox.append(&cards);

    // Mode + core control card
    let mode_card = adw::PreferencesGroup::new();
    mode_card.set_title("Core &amp; Mode");
    mode_card.set_description(Some("Control the mihomo kernel and switch proxy modes"));

    let mode_row = adw::ComboRow::new();
    mode_row.set_title("Proxy Mode");
    mode_row.set_subtitle("Rule uses the ruleset, Global routes all through the selected proxy, Direct bypasses");
    let mode_model = gtk::StringList::new(&["Rule", "Global", "Direct"]);
    mode_row.set_model(Some(&mode_model));
    let cur_mode = state.cfg.read().unwrap().mode.clone();
    mode_row.set_selected(match cur_mode.as_str() {
        "global" => 1,
        "direct" => 2,
        _ => 0,
    });
    {
        let state = state.clone();
        mode_row.connect_selected_notify(move |row| {
            let mode = match row.selected() {
                1 => "global",
                2 => "direct",
                _ => "rule",
            };
            state.cfg.write().unwrap().mode = mode.to_string();
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            let mode_owned = mode.to_string();
            util::detach(async move {
                let api = core.api();
                let _ = api.patch_configs(serde_json::json!({ "mode": mode_owned })).await;
            });
        });
    }
    mode_card.add(&mode_row);

    let tun_row = adw::SwitchRow::new();
    tun_row.set_title("TUN Mode");
    tun_row.set_subtitle("Transparent proxy at the network layer. Requires cap_net_admin on mihomo.");
    tun_row.set_active(state.cfg.read().unwrap().tun_enabled);
    {
        let state = state.clone();
        tun_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            state.cfg.write().unwrap().tun_enabled = enabled;
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move {
                let _ = core.apply_config().await;
            });
        });
    }
    mode_card.add(&tun_row);

    vbox.append(&mode_card);

    // Version/info card
    let info_group = adw::PreferencesGroup::new();
    info_group.set_title("Kernel");
    let version_row = adw::ActionRow::new();
    version_row.set_title("Version");
    version_row.set_subtitle("Kernel not running");
    let version_row_c = version_row.clone();
    info_group.add(&version_row);

    let flush_btn = gtk::Button::with_label("Flush fake-ip cache");
    flush_btn.add_css_class("flat");
    {
        let core = state.core.clone();
        flush_btn.connect_clicked(move |_| {
            let core = core.clone();
            util::detach(async move {
                let api = core.api();
                let _ = api.flush_fake_ip().await;
            });
        });
    }
    let upgrade_geo_btn = gtk::Button::with_label("Update GeoIP databases");
    upgrade_geo_btn.add_css_class("flat");
    {
        let core = state.core.clone();
        upgrade_geo_btn.connect_clicked(move |_| {
            let core = core.clone();
            util::detach(async move {
                let api = core.api();
                let _ = api.upgrade_geo().await;
            });
        });
    }
    let action_row = adw::ActionRow::new();
    action_row.set_title("Maintenance");
    let action_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    action_box.set_valign(gtk::Align::Center);
    action_box.append(&flush_btn);
    action_box.append(&upgrade_geo_btn);
    action_row.add_suffix(&action_box);
    info_group.add(&action_row);

    vbox.append(&info_group);

    // Traffic streams — poll on state change
    let stream_holder: Rc<RefCell<Option<TrafficStreamHandles>>> = Rc::new(RefCell::new(None));
    {
        let state = state.clone();
        let holder = stream_holder.clone();
        let up_value = up_value.clone();
        let down_value = down_value.clone();
        let up_total_value = up_total_value.clone();
        let down_total_value = down_total_value.clone();
        let mem_value = mem_value.clone();
        let conn_value = conn_value.clone();
        let version_row_c = version_row_c.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(800), move || {
            let core = state.core.clone();
            let state_cb = state.clone();
            let holder = holder.clone();
            let up_value = up_value.clone();
            let down_value = down_value.clone();
            let up_total_value = up_total_value.clone();
            let down_total_value = down_total_value.clone();
            let mem_value = mem_value.clone();
            let conn_value = conn_value.clone();
            let version_row_c = version_row_c.clone();
            util::spawn(async move { core.state().await }, move |s| {
                let running = s == CoreState::Running;
                let mut h = holder.borrow_mut();
                if running && h.is_none() {
                    let core = state_cb.core.clone();
                    let api = std::sync::Arc::new(core.api());
                    let (mut traffic_rx, traffic_cancel) = api.stream_traffic();
                    let (mut mem_rx, mem_cancel) = api.stream_memory();
                    let up_value_c = up_value.clone();
                    let down_value_c = down_value.clone();
                    glib::MainContext::default().spawn_local(async move {
                        while let Some(ev) = traffic_rx.recv().await {
                            up_value_c.set_text(&util::format_speed(ev.up));
                            down_value_c.set_text(&util::format_speed(ev.down));
                        }
                    });
                    let mem_value_c = mem_value.clone();
                    glib::MainContext::default().spawn_local(async move {
                        while let Some(ev) = mem_rx.recv().await {
                            mem_value_c.set_text(&util::format_bytes(ev.inuse));
                        }
                    });
                    // Poll connections summary
                    let poll_state = state_cb.clone();
                    let up_total = up_total_value.clone();
                    let down_total = down_total_value.clone();
                    let conn_v = conn_value.clone();
                    let version_row = version_row_c.clone();
                    let stop_flag = Rc::new(RefCell::new(false));
                    let stop_flag_c = stop_flag.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
                        if *stop_flag_c.borrow() { return glib::ControlFlow::Break; }
                        let core = poll_state.core.clone();
                        let up_total = up_total.clone();
                        let down_total = down_total.clone();
                        let conn_v = conn_v.clone();
                        let version_row = version_row.clone();
                        util::spawn(async move {
                            let api = core.api();
                            let conns = api.connections().await.ok();
                            let ver = api.version().await.ok();
                            (conns, ver)
                        }, move |(conns, ver)| {
                            if let Some(c) = conns {
                                up_total.set_text(&util::format_bytes(c.upload_total));
                                down_total.set_text(&util::format_bytes(c.download_total));
                                conn_v.set_text(&c.connections.len().to_string());
                            }
                            if let Some(v) = ver {
                                let tag = if v.meta { " (meta)" } else { "" };
                                version_row.set_subtitle(&format!("mihomo {}{}", v.version, tag));
                            } else {
                                version_row.set_subtitle("Kernel not running");
                            }
                        });
                        glib::ControlFlow::Continue
                    });

                    *h = Some(TrafficStreamHandles {
                        traffic_cancel,
                        mem_cancel,
                        conn_poll_stop: stop_flag,
                    });
                } else if !running && h.is_some() {
                    let handles = h.take().unwrap();
                    let _ = handles.traffic_cancel.try_send(());
                    let _ = handles.mem_cancel.try_send(());
                    *handles.conn_poll_stop.borrow_mut() = true;
                    up_value.set_text("0 B/s");
                    down_value.set_text("0 B/s");
                    mem_value.set_text("-");
                    conn_value.set_text("0");
                    version_row_c.set_subtitle("Kernel not running");
                }
            });
            glib::ControlFlow::Continue
        });
    }

    vbox
}

struct TrafficStreamHandles {
    traffic_cancel: tokio::sync::mpsc::Sender<()>,
    mem_cancel: tokio::sync::mpsc::Sender<()>,
    conn_poll_stop: Rc<RefCell<bool>>,
}

struct StatCard {
    root: gtk::Widget,
    value_label: gtk::Label,
}

fn stat_card(title: &str, initial: &str, icon: &str, accent_class: &str) -> StatCard {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("card");
    card.add_css_class("stat-card");
    card.add_css_class(accent_class);
    card.set_margin_top(4);
    card.set_margin_bottom(4);
    card.set_hexpand(true);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.set_margin_top(14);
    header.set_margin_start(16);
    header.set_margin_end(16);
    let img = gtk::Image::from_icon_name(icon);
    img.set_pixel_size(18);
    img.add_css_class("stat-card-icon");
    let title_lbl = gtk::Label::new(Some(title));
    title_lbl.set_xalign(0.0);
    title_lbl.add_css_class("dim-label");
    title_lbl.add_css_class("caption-heading");
    header.append(&img);
    header.append(&title_lbl);
    card.append(&header);

    let value_label = gtk::Label::new(Some(initial));
    value_label.set_xalign(0.0);
    value_label.set_margin_start(16);
    value_label.set_margin_end(16);
    value_label.set_margin_bottom(16);
    value_label.add_css_class("title-2");
    value_label.add_css_class("stat-card-value");
    card.append(&value_label);

    StatCard {
        root: card.upcast(),
        value_label,
    }
}
