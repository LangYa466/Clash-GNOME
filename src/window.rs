use crate::application::AppState;
use crate::core_manager::CoreState;
use adw::prelude::*;
use gtk::glib;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone)]
pub struct MainWindow {
    inner: Rc<Inner>,
}

struct Inner {
    window: adw::ApplicationWindow,
    header_status: gtk::Label,
    toggle_button: gtk::Button,
    state: Arc<AppState>,
}

impl MainWindow {
    pub fn new(app: &crate::application::ClashGnomeApp, state: Arc<AppState>) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Clash GNOME")
            .default_width(1180)
            .default_height(760)
            .width_request(360)
            .height_request(480)
            .build();
        window.add_css_class("clash-gnome");

        // ===== Header bar =====
        let header = adw::HeaderBar::new();
        header.add_css_class("flat");

        // Status pill on the left of the title
        let header_status = gtk::Label::new(Some("Stopped"));
        header_status.add_css_class("status-pill");
        header_status.add_css_class("status-stopped");
        header.pack_start(&header_status);

        // Toggle button (Start / Stop kernel)
        let toggle_button = gtk::Button::builder()
            .label("Start Core")
            .css_classes(["suggested-action", "pill"])
            .build();
        header.pack_end(&toggle_button);

        // Hamburger menu
        let menu_btn = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .build();
        let menu = gio::Menu::new();
        menu.append(Some("About Clash GNOME"), Some("app.about"));
        menu.append(Some("Quit"), Some("app.quit"));
        menu_btn.set_menu_model(Some(&menu));
        header.pack_end(&menu_btn);

        // ===== Sidebar =====
        let view_stack = adw::ViewStack::new();
        view_stack.set_vexpand(true);
        view_stack.set_hexpand(true);

        add_pages(&view_stack, &state);

        // For a dedicated sidebar list feel we use a NavigationSplitView with a
        // custom sidebar containing the view switcher stacked vertically.
        let sidebar_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_box.set_width_request(240);
        sidebar_box.add_css_class("sidebar");

        // Sidebar top: brand / logo
        let brand = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        brand.set_margin_top(18);
        brand.set_margin_bottom(18);
        brand.set_margin_start(18);
        brand.set_margin_end(18);
        let logo = gtk::Image::from_icon_name("network-vpn-symbolic");
        logo.set_pixel_size(28);
        logo.add_css_class("accent");
        let brand_label = gtk::Label::new(Some("Clash GNOME"));
        brand_label.add_css_class("title-3");
        brand_label.set_xalign(0.0);
        brand.append(&logo);
        brand.append(&brand_label);
        sidebar_box.append(&brand);

        // Sidebar list of pages
        let listbox = build_sidebar_list(&view_stack);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&listbox)
            .build();
        sidebar_box.append(&scrolled);

        // The view switcher sits in header when narrow (mobile); wide mode: hidden
        // Content area
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&view_stack));

        let split = adw::NavigationSplitView::new();
        let sidebar_page = adw::NavigationPage::builder()
            .title("Clash GNOME")
            .child(&sidebar_box)
            .build();
        let content_page = adw::NavigationPage::builder()
            .title("Content")
            .child(&toolbar_view)
            .build();
        split.set_sidebar(Some(&sidebar_page));
        split.set_content(Some(&content_page));
        split.set_min_sidebar_width(220.0);
        split.set_max_sidebar_width(280.0);

        // Breakpoint: collapse on narrow
        let breakpoint = adw::Breakpoint::new(
            adw::BreakpointCondition::new_length(
                adw::BreakpointConditionLengthType::MaxWidth,
                720.0,
                adw::LengthUnit::Sp,
            ),
        );
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(breakpoint);

        window.set_content(Some(&split));

        // Bind toggle button behavior
        let inner = Rc::new(Inner {
            window: window.clone(),
            header_status: header_status.clone(),
            toggle_button: toggle_button.clone(),
            state: state.clone(),
        });
        let this = MainWindow { inner };
        this.wire_toggle_button();
        this.start_state_watcher();
        this
    }

    pub fn present(&self) {
        self.inner.window.present();
    }

    fn wire_toggle_button(&self) {
        let state = self.inner.state.clone();
        let btn = self.inner.toggle_button.clone();
        let window = self.inner.window.clone();
        btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let core = state.core.clone();
            let window = window.clone();
            let b = b.clone();
            crate::util::spawn(
                async move {
                    let s = core.state().await;
                    if s == CoreState::Running || s == CoreState::Starting {
                        core.stop().await
                    } else {
                        core.start().await
                    }
                },
                move |res| {
                    b.set_sensitive(true);
                    if let Err(e) = res {
                        let dlg = adw::AlertDialog::builder()
                            .heading("Core operation failed")
                            .body(format!("{e}"))
                            .build();
                        dlg.add_response("ok", "OK");
                        dlg.present(Some(&window));
                    }
                },
            );
        });
    }

    fn start_state_watcher(&self) {
        let core = self.inner.state.core.clone();
        let label = self.inner.header_status.clone();
        let btn = self.inner.toggle_button.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(600), move || {
            let core = core.clone();
            let label = label.clone();
            let btn = btn.clone();
            crate::util::spawn(async move { core.state().await }, move |s| {
                let (text, klass, btn_label, btn_class) = match s {
                    CoreState::Stopped => ("Stopped", "status-stopped", "Start Core", "suggested-action"),
                    CoreState::Starting => ("Starting…", "status-starting", "Starting…", "suggested-action"),
                    CoreState::Running => ("Running", "status-running", "Stop Core", "destructive-action"),
                    CoreState::Stopping => ("Stopping…", "status-starting", "Stopping…", "destructive-action"),
                    CoreState::Failed => ("Failed", "status-failed", "Start Core", "suggested-action"),
                };
                label.set_text(text);
                for c in ["status-stopped", "status-starting", "status-running", "status-failed"] {
                    label.remove_css_class(c);
                }
                label.add_css_class(klass);
                btn.set_label(btn_label);
                for c in ["suggested-action", "destructive-action"] {
                    btn.remove_css_class(c);
                }
                btn.add_css_class(btn_class);
            });
            glib::ControlFlow::Continue
        });
    }
}

use gtk::gio;

fn add_pages(view_stack: &adw::ViewStack, state: &Arc<AppState>) {
    let dashboard = crate::pages::dashboard::build(state.clone());
    let proxies = crate::pages::proxies::build(state.clone());
    let rules = crate::pages::rules::build(state.clone());
    let connections = crate::pages::connections::build(state.clone());
    let subscriptions = crate::pages::subscriptions::build(state.clone());
    let logs = crate::pages::logs::build(state.clone());
    let settings = crate::pages::settings::build(state.clone());

    let p = view_stack.add_titled_with_icon(&dashboard, Some("dashboard"), "Dashboard", "utilities-system-monitor-symbolic");
    p.set_visible(true);
    view_stack.add_titled_with_icon(&proxies, Some("proxies"), "Proxies", "network-workgroup-symbolic");
    view_stack.add_titled_with_icon(&rules, Some("rules"), "Rules", "view-list-symbolic");
    view_stack.add_titled_with_icon(&connections, Some("connections"), "Connections", "network-transmit-receive-symbolic");
    view_stack.add_titled_with_icon(&subscriptions, Some("subscriptions"), "Subscriptions", "folder-download-symbolic");
    view_stack.add_titled_with_icon(&logs, Some("logs"), "Logs", "utilities-terminal-symbolic");
    view_stack.add_titled_with_icon(&settings, Some("settings"), "Settings", "preferences-system-symbolic");
}

fn build_sidebar_list(view_stack: &adw::ViewStack) -> gtk::ListBox {
    let listbox = gtk::ListBox::new();
    listbox.set_selection_mode(gtk::SelectionMode::Single);
    listbox.add_css_class("navigation-sidebar");

    let entries: &[(&str, &str, &str)] = &[
        ("dashboard", "Dashboard", "utilities-system-monitor-symbolic"),
        ("proxies", "Proxies", "network-workgroup-symbolic"),
        ("rules", "Rules", "view-list-symbolic"),
        ("connections", "Connections", "network-transmit-receive-symbolic"),
        ("subscriptions", "Subscriptions", "folder-download-symbolic"),
        ("logs", "Logs", "utilities-terminal-symbolic"),
        ("settings", "Settings", "preferences-system-symbolic"),
    ];

    for (name, label, icon) in entries {
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(name);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hbox.set_margin_top(6);
        hbox.set_margin_bottom(6);
        hbox.set_margin_start(14);
        hbox.set_margin_end(14);
        let img = gtk::Image::from_icon_name(icon);
        img.set_pixel_size(18);
        let lbl = gtk::Label::new(Some(label));
        lbl.set_xalign(0.0);
        hbox.append(&img);
        hbox.append(&lbl);
        row.set_child(Some(&hbox));
        listbox.append(&row);
    }

    let vs = view_stack.clone();
    listbox.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            let name = r.widget_name().to_string();
            vs.set_visible_child_name(&name);
        }
    });

    // Select the first row by default
    if let Some(first) = listbox.row_at_index(0) {
        listbox.select_row(Some(&first));
    }

    listbox
}
