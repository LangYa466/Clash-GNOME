use crate::application::AppState;
use crate::config::Subscription;
use crate::{subscription, util};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;
use std::sync::Arc;

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toolbar.set_margin_top(16);
    toolbar.set_margin_start(20);
    toolbar.set_margin_end(20);
    toolbar.set_margin_bottom(10);

    let title = gtk::Label::new(Some("Subscriptions"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    toolbar.append(&title);

    let add_menu = gio::Menu::new();
    add_menu.append(Some("From URL…"), Some("subs.add-url"));
    add_menu.append(Some("From local file…"), Some("subs.add-file"));
    let add_btn = gtk::MenuButton::builder()
        .label("Add Subscription")
        .icon_name("list-add-symbolic")
        .always_show_arrow(true)
        .menu_model(&add_menu)
        .css_classes(["suggested-action", "pill"])
        .build();
    toolbar.append(&add_btn);

    root.append(&toolbar);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(1100)
        .tightening_threshold(900)
        .build();
    let list_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    list_box.set_margin_top(6);
    list_box.set_margin_bottom(24);
    list_box.set_margin_start(20);
    list_box.set_margin_end(20);
    clamp.set_child(Some(&list_box));
    scrolled.set_child(Some(&clamp));

    let empty = adw::StatusPage::builder()
        .icon_name("folder-download-symbolic")
        .title("No subscriptions yet")
        .description("Add a Clash/mihomo YAML subscription URL to get started.")
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&empty, Some("empty"));
    root.append(&stack);

    let list_box = Rc::new(list_box);

    type RenderRef = Rc<std::cell::RefCell<Option<Rc<dyn Fn()>>>>;
    let render_cell: RenderRef = Rc::new(std::cell::RefCell::new(None));
    let render: Rc<dyn Fn()> = {
        let state = state.clone();
        let list_box = list_box.clone();
        let stack = stack.clone();
        let render_cell = render_cell.clone();
        Rc::new(move || {
            while let Some(c) = list_box.first_child() {
                list_box.remove(&c);
            }
            let subs = state.cfg.read().unwrap().subscriptions.clone();
            let active = state.cfg.read().unwrap().active_subscription.clone();
            if subs.is_empty() {
                stack.set_visible_child_name("empty");
                return;
            }
            stack.set_visible_child_name("list");
            let render_ref = render_cell.borrow().clone().expect("render initialized");
            for sub in subs {
                let is_active = active.as_ref().map(|s| s == &sub.id).unwrap_or(false);
                let row = sub_card(&sub, is_active, state.clone(), render_ref.clone());
                list_box.append(&row);
            }
        })
    };
    *render_cell.borrow_mut() = Some(render.clone());
    render();

    {
        let action_group = gio::SimpleActionGroup::new();

        let add_url_action = gio::SimpleAction::new("add-url", None);
        {
            let state = state.clone();
            let render = render.clone();
            let root_c = root.clone();
            add_url_action.connect_activate(move |_, _| {
                open_add_url_dialog(&root_c, state.clone(), render.clone());
            });
        }
        action_group.add_action(&add_url_action);

        let add_file_action = gio::SimpleAction::new("add-file", None);
        {
            let state = state.clone();
            let render = render.clone();
            let root_c = root.clone();
            add_file_action.connect_activate(move |_, _| {
                open_add_file_dialog(&root_c, state.clone(), render.clone());
            });
        }
        action_group.add_action(&add_file_action);

        root.insert_action_group("subs", Some(&action_group));
    }

    // Public re-render trigger via glib property change is complex; expose via periodic refresh
    // of remote quota. Rebind on demand:
    {
        let render = render.clone();
        let state = state.clone();
        // Refresh cards every 20s to update quota display live
        let last_len = Rc::new(std::cell::Cell::new(state.cfg.read().unwrap().subscriptions.len()));
        glib::timeout_add_seconds_local(3, move || {
            let n = state.cfg.read().unwrap().subscriptions.len();
            if n != last_len.get() {
                last_len.set(n);
                render();
            }
            glib::ControlFlow::Continue
        });
    }

    root.upcast()
}

fn sub_card(sub: &Subscription, is_active: bool, state: Arc<AppState>, render: Rc<dyn Fn()>) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("card");
    card.add_css_class("sub-card");
    if is_active {
        card.add_css_class("sub-card-active");
    }

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.set_margin_top(14);
    header.set_margin_start(16);
    header.set_margin_end(16);

    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let name = gtk::Label::new(Some(&sub.name));
    name.add_css_class("title-3");
    name.set_xalign(0.0);
    let url = gtk::Label::new(Some(&sub.url));
    url.add_css_class("dim-label");
    url.add_css_class("caption");
    url.set_xalign(0.0);
    url.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    title_box.append(&name);
    title_box.append(&url);
    title_box.set_hexpand(true);
    header.append(&title_box);

    if is_active {
        let active_badge = gtk::Label::new(Some("Active"));
        active_badge.add_css_class("active-badge");
        header.append(&active_badge);
    }
    card.append(&header);

    // Quota bar
    let quota_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    quota_box.set_margin_start(16);
    quota_box.set_margin_end(16);
    let quota_lbl = gtk::Label::new(None);
    quota_lbl.set_xalign(0.0);
    quota_lbl.add_css_class("caption");
    let used = sub.upload + sub.download;
    let percent = if sub.total > 0 { (used as f64 / sub.total as f64).clamp(0.0, 1.0) } else { 0.0 };
    let expire_text = sub.expire.map(|e| e.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "-".into());
    quota_lbl.set_text(&format!(
        "{} / {} used · Expires {}",
        util::format_bytes(used),
        if sub.total > 0 { util::format_bytes(sub.total) } else { "∞".into() },
        expire_text
    ));
    let bar = gtk::ProgressBar::new();
    bar.set_fraction(percent);
    if percent > 0.9 { bar.add_css_class("warning"); }
    quota_box.append(&quota_lbl);
    quota_box.append(&bar);
    card.append(&quota_box);

    // Auto-update row
    let au_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    au_row.set_margin_start(16);
    au_row.set_margin_end(16);
    au_row.set_margin_top(4);
    let au_check = gtk::CheckButton::builder()
        .label("Auto-update every")
        .active(sub.auto_update)
        .build();
    let au_spin = gtk::SpinButton::with_range(1.0, 999.0, 1.0);
    au_spin.set_value(sub.auto_update_value as f64);
    au_spin.set_sensitive(sub.auto_update);
    au_spin.set_width_chars(4);
    let au_unit = gtk::DropDown::from_strings(&["minutes", "hours", "days"]);
    au_unit.set_selected(match sub.auto_update_unit {
        crate::config::IntervalUnit::Minutes => 0,
        crate::config::IntervalUnit::Hours => 1,
        crate::config::IntervalUnit::Days => 2,
    });
    au_unit.set_sensitive(sub.auto_update);
    au_row.append(&au_check);
    au_row.append(&au_spin);
    au_row.append(&au_unit);

    let proxy_check = gtk::CheckButton::builder()
        .label("Update through proxy")
        .active(sub.use_proxy_for_update)
        .halign(gtk::Align::End)
        .hexpand(true)
        .tooltip_text("When on, fetching the subscription goes through the running mihomo mixed port. When off, requests bypass the proxy.")
        .build();
    au_row.append(&proxy_check);

    {
        let state = state.clone();
        let sub_id_c = sub.id.clone();
        proxy_check.connect_toggled(move |c| {
            if let Some(sub) = state.cfg.write().unwrap().subscriptions.iter_mut().find(|s| s.id == sub_id_c) {
                sub.use_proxy_for_update = c.is_active();
            }
            let _ = crate::config::persist(&state.cfg);
        });
    }

    card.append(&au_row);

    {
        let state = state.clone();
        let sub_id_c = sub.id.clone();
        let au_spin_c = au_spin.clone();
        let au_unit_c = au_unit.clone();
        au_check.connect_toggled(move |c| {
            let on = c.is_active();
            au_spin_c.set_sensitive(on);
            au_unit_c.set_sensitive(on);
            if let Some(s) = state.cfg.write().unwrap().subscriptions.iter_mut().find(|s| s.id == sub_id_c) {
                s.auto_update = on;
            }
            let _ = crate::config::persist(&state.cfg);
        });
    }
    {
        let state = state.clone();
        let sub_id_c = sub.id.clone();
        au_spin.connect_value_changed(move |s| {
            let v = s.value() as u32;
            if let Some(sub) = state.cfg.write().unwrap().subscriptions.iter_mut().find(|s| s.id == sub_id_c) {
                sub.auto_update_value = v.max(1);
            }
            let _ = crate::config::persist(&state.cfg);
        });
    }
    {
        let state = state.clone();
        let sub_id_c = sub.id.clone();
        au_unit.connect_selected_notify(move |dd| {
            let u = match dd.selected() {
                0 => crate::config::IntervalUnit::Minutes,
                2 => crate::config::IntervalUnit::Days,
                _ => crate::config::IntervalUnit::Hours,
            };
            if let Some(sub) = state.cfg.write().unwrap().subscriptions.iter_mut().find(|s| s.id == sub_id_c) {
                sub.auto_update_unit = u;
            }
            let _ = crate::config::persist(&state.cfg);
        });
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_top(6);
    footer.set_margin_bottom(14);
    footer.set_margin_start(16);
    footer.set_margin_end(16);

    let updated_text = sub.updated_at.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "Never".into());
    let updated_lbl = gtk::Label::new(Some(&format!("Updated: {}", updated_text)));
    updated_lbl.add_css_class("dim-label");
    updated_lbl.add_css_class("caption");
    updated_lbl.set_xalign(0.0);
    updated_lbl.set_hexpand(true);
    footer.append(&updated_lbl);

    let use_btn = gtk::Button::builder()
        .label(if is_active { "In Use" } else { "Use" })
        .css_classes([if is_active { "flat" } else { "suggested-action" }])
        .sensitive(!is_active)
        .build();
    let update_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    update_btn.set_tooltip_text(Some("Update"));
    update_btn.add_css_class("flat");
    let delete_btn = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_btn.set_tooltip_text(Some("Delete"));
    delete_btn.add_css_class("flat");
    delete_btn.add_css_class("destructive-action");
    footer.append(&use_btn);
    footer.append(&update_btn);
    footer.append(&delete_btn);
    card.append(&footer);

    let sub_id = sub.id.clone();
    let sub_c = sub.clone();

    {
        let state = state.clone();
        let sub_id = sub_id.clone();
        let render = render.clone();
        use_btn.connect_clicked(move |_| {
            state.cfg.write().unwrap().active_subscription = Some(sub_id.clone());
            let _ = crate::config::persist(&state.cfg);
            let core = state.core.clone();
            util::detach(async move {
                let _ = core.apply_config().await;
            });
            render();
        });
    }
    {
        let state = state.clone();
        let render = render.clone();
        update_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let state = state.clone();
            let sub_c = sub_c.clone();
            let sub_id_c = sub_c.id.clone();
            let b_c = b.clone();
            let render = render.clone();
            let (ua, proxy_url) = {
                let cfg = state.cfg.read().unwrap();
                let px = if sub_c.use_proxy_for_update {
                    Some(format!("http://{}:{}", cfg.api_host, cfg.mixed_port))
                } else { None };
                (cfg.subscription_user_agent.clone(), px)
            };
            util::spawn(async move {
                subscription::fetch_with_proxy(&sub_c, &ua, proxy_url.as_deref()).await
            }, move |res| {
                b_c.set_sensitive(true);
                if let Ok(out) = res {
                    let mut g = state.cfg.write().unwrap();
                    if let Some(s) = g.subscriptions.iter_mut().find(|s| s.id == sub_id_c) {
                        s.upload = out.upload;
                        s.download = out.download;
                        s.total = out.total;
                        s.expire = out.expire;
                        s.updated_at = Some(chrono::Utc::now());
                    }
                    drop(g);
                    let _ = crate::config::persist(&state.cfg);
                    let core = state.core.clone();
                    util::detach(async move {
                        let _ = core.apply_config().await;
                    });
                    render();
                }
            });
        });
    }
    {
        let state = state.clone();
        let sub_id = sub_id.clone();
        let render = render.clone();
        delete_btn.connect_clicked(move |_| {
            {
                let mut g = state.cfg.write().unwrap();
                g.subscriptions.retain(|s| s.id != sub_id);
                if g.active_subscription.as_ref() == Some(&sub_id) {
                    g.active_subscription = None;
                }
            }
            let _ = crate::config::persist(&state.cfg);
            subscription::delete_local(&sub_id);
            render();
        });
    }

    card.upcast()
}

fn open_add_url_dialog(parent: &gtk::Box, state: Arc<AppState>, render: Rc<dyn Fn()>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Add Subscription");
    dialog.set_content_width(480);

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 16);
    box_.set_margin_top(20);
    box_.set_margin_bottom(20);
    box_.set_margin_start(20);
    box_.set_margin_end(20);

    let name_group = adw::PreferencesGroup::new();
    let name_row = adw::EntryRow::new();
    name_row.set_title("Name");
    name_group.add(&name_row);
    let url_row = adw::EntryRow::new();
    url_row.set_title("Subscription URL");
    name_group.add(&url_row);
    box_.append(&name_group);

    let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    button_row.set_halign(gtk::Align::End);
    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::builder()
        .label("Add")
        .css_classes(["suggested-action"])
        .build();
    button_row.append(&cancel_btn);
    button_row.append(&save_btn);
    box_.append(&button_row);

    toolbar_view.set_content(Some(&box_));
    dialog.set_child(Some(&toolbar_view));

    let dialog_c = dialog.clone();
    cancel_btn.connect_clicked(move |_| { dialog_c.close(); });

    let dialog_c = dialog.clone();
    let render_c = render.clone();
    save_btn.connect_clicked(move |b| {
        let name = name_row.text().trim().to_string();
        let url = url_row.text().trim().to_string();
        if name.is_empty() || url.is_empty() { return; }
        if url::Url::parse(&url).is_err() { return; }
        b.set_sensitive(false);
        let sub = Subscription {
            id: subscription::new_id(),
            name,
            url,
            updated_at: None,
            upload: 0,
            download: 0,
            total: 0,
            expire: None,
            auto_update: false,
            auto_update_value: 24,
            auto_update_unit: crate::config::IntervalUnit::Hours,
            use_proxy_for_update: false,
        };
        let state = state.clone();
        let dialog_c2 = dialog_c.clone();
        let render_cc = render_c.clone();
        let b_c = b.clone();
        let ua = state.cfg.read().unwrap().subscription_user_agent.clone();
        util::spawn(async move {
            let out = subscription::fetch(&sub, &ua).await;
            (sub, out)
        }, move |(mut sub, out)| {
            b_c.set_sensitive(true);
            match out {
                Ok(o) => {
                    sub.upload = o.upload;
                    sub.download = o.download;
                    sub.total = o.total;
                    sub.expire = o.expire;
                    sub.updated_at = Some(chrono::Utc::now());
                    {
                        let mut g = state.cfg.write().unwrap();
                        g.subscriptions.push(sub);
                        if g.active_subscription.is_none() {
                            g.active_subscription = g.subscriptions.last().map(|s| s.id.clone());
                        }
                    }
                    let _ = crate::config::persist(&state.cfg);
                    let core = state.core.clone();
                    util::detach(async move { let _ = core.apply_config().await; });
                    render_cc();
                    dialog_c2.close();
                }
                Err(e) => {
                    let inner = adw::AlertDialog::builder()
                        .heading("Failed to fetch subscription")
                        .body(format!("{e}"))
                        .build();
                    inner.add_response("ok", "OK");
                    inner.present(Some(&dialog_c2));
                }
            }
        });
    });

    dialog.present(Some(parent));
}

fn open_add_file_dialog(parent: &gtk::Box, state: Arc<AppState>, render: Rc<dyn Fn()>) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Clash / mihomo YAML"));
    filter.add_pattern("*.yaml");
    filter.add_pattern("*.yml");
    filter.add_mime_type("application/x-yaml");
    filter.add_mime_type("text/yaml");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);

    let dialog = gtk::FileDialog::builder()
        .title("Import subscription from file")
        .modal(true)
        .filters(&filters)
        .default_filter(&filter)
        .build();

    let win = parent.root().and_then(|r| r.downcast::<gtk::Window>().ok());
    let parent_c = parent.clone();
    dialog.open(win.as_ref(), None::<&gio::Cancellable>, move |res| {
        let file = match res {
            Ok(f) => f,
            Err(_) => return,
        };
        let Some(path) = file.path() else { return; };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                error_dialog(&parent_c, "Failed to read file", &format!("{e}"));
                return;
            }
        };
        let v: Result<serde_yaml::Value, _> = serde_yaml::from_str(&text);
        match v {
            Ok(serde_yaml::Value::Mapping(_)) => {}
            Ok(_) => {
                error_dialog(&parent_c, "Invalid YAML", "The file's top level is not a mapping");
                return;
            }
            Err(e) => {
                error_dialog(&parent_c, "Invalid YAML", &format!("{e}"));
                return;
            }
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("local").to_string();
        let sub = Subscription {
            id: subscription::new_id(),
            name: stem,
            url: format!("file://{}", path.display()),
            updated_at: Some(chrono::Utc::now()),
            upload: 0,
            download: 0,
            total: 0,
            expire: None,
            auto_update: false,
            auto_update_value: 24,
            auto_update_unit: crate::config::IntervalUnit::Hours,
            use_proxy_for_update: false,
        };
        let dest = crate::config::subscriptions_dir().join(format!("{}.yaml", sub.id));
        if let Err(e) = std::fs::write(&dest, &text) {
            error_dialog(&parent_c, "Failed to save subscription", &format!("{e}"));
            return;
        }

        {
            let mut g = state.cfg.write().unwrap();
            g.subscriptions.push(sub);
            if g.active_subscription.is_none() {
                g.active_subscription = g.subscriptions.last().map(|s| s.id.clone());
            }
        }
        let _ = crate::config::persist(&state.cfg);
        let core = state.core.clone();
        util::detach(async move { let _ = core.apply_config().await; });
        render();
    });
}

fn error_dialog(parent: &gtk::Box, heading: &str, body: &str) {
    let dlg = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dlg.add_response("ok", "OK");
    dlg.present(Some(parent));
}
