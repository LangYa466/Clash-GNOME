use crate::application::AppState;
use crate::util;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    Time,
    NowUp,
    NowDown,
    AllUp,
    AllDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkFilter { All, Tcp, Udp }

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toolbar.set_margin_top(16);
    toolbar.set_margin_start(20);
    toolbar.set_margin_end(20);
    toolbar.set_margin_bottom(10);

    let title = gtk::Label::new(Some("Active Connections"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    toolbar.append(&title);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Filter host / rule / process...")
        .hexpand(true)
        .build();
    toolbar.append(&search);

    let net_dd = gtk::DropDown::from_strings(&["All", "TCP", "UDP"]);
    net_dd.set_tooltip_text(Some("Network filter"));
    toolbar.append(&net_dd);

    let sort_dd = gtk::DropDown::from_strings(&["Time", "Now Up", "Now Down", "Total Up", "Total Down"]);
    sort_dd.set_tooltip_text(Some("Sort by"));
    toolbar.append(&sort_dd);

    let close_all_btn = gtk::Button::builder()
        .label("Close All")
        .css_classes(["destructive-action", "pill"])
        .build();
    toolbar.append(&close_all_btn);

    let refresh_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.add_css_class("flat");
    toolbar.append(&refresh_btn);

    root.append(&toolbar);

    let summary = gtk::Label::new(Some("0 active | Up 0 B | Down 0 B"));
    summary.add_css_class("dim-label");
    summary.add_css_class("caption");
    summary.set_xalign(0.0);
    summary.set_margin_start(20);
    summary.set_margin_end(20);
    root.append(&summary);

    let store = gio::ListStore::new::<ConnObject>();
    let sort_key = Rc::new(std::cell::Cell::new(SortKey::Time));
    let net_filter = Rc::new(std::cell::Cell::new(NetworkFilter::All));
    let search_text = Rc::new(RefCell::new(String::new()));

    let filter = gtk::CustomFilter::new({
        let net_filter = net_filter.clone();
        let search_text = search_text.clone();
        move |obj| {
            let c = obj.downcast_ref::<ConnObject>().unwrap();
            match net_filter.get() {
                NetworkFilter::Tcp if c.network().to_lowercase() != "tcp" => return false,
                NetworkFilter::Udp if c.network().to_lowercase() != "udp" => return false,
                _ => {}
            }
            let q = search_text.borrow();
            if q.is_empty() { return true; }
            let q = q.to_lowercase();
            c.host().to_lowercase().contains(&q)
                || c.destination().to_lowercase().contains(&q)
                || c.chains().to_lowercase().contains(&q)
                || c.rule().to_lowercase().contains(&q)
                || c.process().to_lowercase().contains(&q)
        }
    });
    let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    let selection = gtk::NoSelection::new(Some(filter_model.clone()));

    let factory = gtk::SignalListItemFactory::new();
    let state_setup = state.clone();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
        row.set_margin_top(10);
        row.set_margin_bottom(10);
        row.set_margin_start(20);
        row.set_margin_end(20);

        let l1 = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let proto_lbl = gtk::Label::new(None);
        proto_lbl.add_css_class("proto-badge");
        proto_lbl.set_width_request(48);
        let host = gtk::Label::new(None);
        host.set_xalign(0.0);
        host.set_hexpand(true);
        host.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        host.add_css_class("body");
        let now_up = gtk::Label::new(None);
        now_up.add_css_class("caption");
        now_up.add_css_class("dim-label");
        now_up.set_width_chars(11);
        now_up.set_xalign(1.0);
        let now_down = gtk::Label::new(None);
        now_down.add_css_class("caption");
        now_down.add_css_class("dim-label");
        now_down.set_width_chars(11);
        now_down.set_xalign(1.0);
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.add_css_class("circular");
        close_btn.set_tooltip_text(Some("Close this connection"));
        l1.append(&proto_lbl);
        l1.append(&host);
        l1.append(&now_up);
        l1.append(&now_down);
        l1.append(&close_btn);

        let l2 = gtk::Label::new(None);
        l2.set_xalign(0.0);
        l2.add_css_class("dim-label");
        l2.add_css_class("caption");
        l2.set_ellipsize(gtk::pango::EllipsizeMode::End);

        row.append(&l1);
        row.append(&l2);
        item.set_child(Some(&row));

        // Row-level click -> detail dialog
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        let item_weak = item.downgrade();
        let row_c = row.clone();
        gesture.connect_pressed(move |_, n_press, _x, _y| {
            if n_press < 2 { return; }
            let Some(item) = item_weak.upgrade() else { return; };
            let Some(obj) = item.item().and_downcast::<ConnObject>() else { return; };
            let root_win = row_c.root().and_then(|r| r.downcast::<gtk::Window>().ok());
            show_detail_dialog(root_win.as_ref(), &obj);
        });
        row.add_controller(gesture);

        // Close button
        let item_weak = item.downgrade();
        let core = state_setup.core.clone();
        close_btn.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else { return; };
            let Some(obj) = item.item().and_downcast::<ConnObject>() else { return; };
            let id = obj.id();
            let core = core.clone();
            util::detach(async move {
                let _ = core.api().close_connection(&id).await;
            });
        });
    });
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<ConnObject>().unwrap();
        let row = item.child().and_downcast::<gtk::Box>().unwrap();
        let l1 = row.first_child().and_then(|c| c.downcast::<gtk::Box>().ok()).unwrap();
        let proto = l1.first_child().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let host = proto.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let now_up = host.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let now_down = now_up.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let l2 = l1.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();

        let net = obj.network().to_uppercase();
        proto.set_text(&net);
        for c in ["proto-tcp", "proto-udp", "proto-other"] { proto.remove_css_class(c); }
        proto.add_css_class(match net.as_str() {
            "TCP" => "proto-tcp",
            "UDP" => "proto-udp",
            _ => "proto-other",
        });

        let display = if obj.host().is_empty() { obj.destination() } else { obj.host() };
        host.set_text(&display);
        now_up.set_text(&format!("up {}", util::format_speed(obj.now_up())));
        now_down.set_text(&format!("dn {}", util::format_speed(obj.now_down())));

        let mut meta = String::new();
        if !obj.chains().is_empty() { meta.push_str(&format!("Chain: {} | ", obj.chains())); }
        if !obj.rule().is_empty() { meta.push_str(&format!("Rule: {} | ", obj.rule())); }
        let proc_name = obj.process();
        meta.push_str(&format!("Process: {} | Total ^{} v{}",
            if proc_name.is_empty() { "-" } else { proc_name.as_str() },
            util::format_bytes(obj.upload()),
            util::format_bytes(obj.download())));
        l2.set_text(&meta);
    });

    let listview = gtk::ListView::new(Some(selection.clone()), Some(factory));
    listview.add_css_class("conn-list");

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&listview)
        .build();

    let empty = adw::StatusPage::builder()
        .icon_name("network-transmit-receive-symbolic")
        .title("No active connections")
        .description("Start the core and generate some traffic.")
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&empty, Some("empty"));
    root.append(&stack);

    // Speed tracker across polls
    let prev_snapshot: Rc<RefCell<HashMap<String, (u64, u64, Instant)>>> = Rc::new(RefCell::new(HashMap::new()));

    let refresh = {
        let core = state.core.clone();
        let store = store.clone();
        let stack = stack.clone();
        let summary = summary.clone();
        let sort_key = sort_key.clone();
        let prev_snapshot = prev_snapshot.clone();
        Rc::new(move || {
            let core = core.clone();
            let store = store.clone();
            let stack = stack.clone();
            let summary = summary.clone();
            let sort_key = sort_key.clone();
            let prev_snapshot = prev_snapshot.clone();
            util::spawn(async move {
                let api = core.api();
                api.connections().await
            }, move |res| {
                match res {
                    Ok(r) => {
                        let now_ts = Instant::now();
                        let mut prev = prev_snapshot.borrow_mut();
                        let mut new_snap = HashMap::new();
                        let mut enriched: Vec<(crate::api::ConnectionItem, u64, u64)> = Vec::with_capacity(r.connections.len());
                        for c in &r.connections {
                            let (nup, ndown) = match prev.get(&c.id) {
                                Some((pu, pd, pt)) => {
                                    let dt = now_ts.duration_since(*pt).as_secs_f64().max(0.001);
                                    let du = c.upload.saturating_sub(*pu) as f64 / dt;
                                    let dd = c.download.saturating_sub(*pd) as f64 / dt;
                                    (du.round() as u64, dd.round() as u64)
                                }
                                None => (0, 0),
                            };
                            new_snap.insert(c.id.clone(), (c.upload, c.download, now_ts));
                            enriched.push((c.clone(), nup, ndown));
                        }
                        *prev = new_snap;

                        match sort_key.get() {
                            SortKey::Time => enriched.sort_by(|a, b| b.0.start.cmp(&a.0.start)),
                            SortKey::NowUp => enriched.sort_by_key(|e| std::cmp::Reverse(e.1)),
                            SortKey::NowDown => enriched.sort_by_key(|e| std::cmp::Reverse(e.2)),
                            SortKey::AllUp => enriched.sort_by_key(|e| std::cmp::Reverse(e.0.upload)),
                            SortKey::AllDown => enriched.sort_by_key(|e| std::cmp::Reverse(e.0.download)),
                        }

                        store.remove_all();
                        for (c, nup, ndown) in &enriched {
                            store.append(&ConnObject::from_api(c, *nup, *ndown));
                        }
                        summary.set_text(&format!(
                            "{} active | Up {} | Down {}",
                            enriched.len(),
                            util::format_bytes(r.upload_total),
                            util::format_bytes(r.download_total),
                        ));
                        if enriched.is_empty() {
                            stack.set_visible_child_name("empty");
                        } else {
                            stack.set_visible_child_name("list");
                        }
                    }
                    Err(_) => stack.set_visible_child_name("empty"),
                }
            });
        })
    };
    {
        let r = refresh.clone();
        refresh_btn.connect_clicked(move |_| r());
    }
    {
        let filter_c = filter.clone();
        let search_text = search_text.clone();
        search.connect_search_changed(move |s| {
            *search_text.borrow_mut() = s.text().to_string();
            filter_c.changed(gtk::FilterChange::Different);
        });
    }
    {
        let net_filter = net_filter.clone();
        let filter = filter.clone();
        net_dd.connect_selected_notify(move |dd| {
            net_filter.set(match dd.selected() {
                1 => NetworkFilter::Tcp,
                2 => NetworkFilter::Udp,
                _ => NetworkFilter::All,
            });
            filter.changed(gtk::FilterChange::Different);
        });
    }
    {
        let sort_key = sort_key.clone();
        let r = refresh.clone();
        sort_dd.connect_selected_notify(move |dd| {
            sort_key.set(match dd.selected() {
                1 => SortKey::NowUp,
                2 => SortKey::NowDown,
                3 => SortKey::AllUp,
                4 => SortKey::AllDown,
                _ => SortKey::Time,
            });
            r();
        });
    }
    {
        let core = state.core.clone();
        let refresh = refresh.clone();
        close_all_btn.connect_clicked(move |_| {
            let core = core.clone();
            let refresh = refresh.clone();
            util::spawn(async move {
                let api = core.api();
                api.close_all_connections().await
            }, move |_| { refresh(); });
        });
    }
    {
        let r = refresh.clone();
        glib::timeout_add_seconds_local(1, move || {
            r();
            glib::ControlFlow::Continue
        });
        refresh();
    }

    root.upcast()
}

fn show_detail_dialog(parent: Option<&gtk::Window>, obj: &ConnObject) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Connection detail");
    dialog.set_content_width(560);

    let tv = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    tv.add_top_bar(&header);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("Connection");
    add_kv(&group, "ID", &obj.id());
    add_kv(&group, "Network", &obj.network());
    add_kv(&group, "Type", &obj.ty());
    let host_s = obj.host();
    add_kv(&group, "Host", if host_s.is_empty() { "-" } else { host_s.as_str() });
    add_kv(&group, "Source", &obj.source());
    add_kv(&group, "Destination", &obj.destination());
    let proc_s = obj.process();
    add_kv(&group, "Process", if proc_s.is_empty() { "-" } else { proc_s.as_str() });
    page.add(&group);

    let route = adw::PreferencesGroup::new();
    route.set_title("Routing");
    add_kv(&route, "Chain", &obj.chains());
    add_kv(&route, "Rule", &obj.rule());
    page.add(&route);

    let tr = adw::PreferencesGroup::new();
    tr.set_title("Transfer");
    add_kv(&tr, "Upload total", &util::format_bytes(obj.upload()));
    add_kv(&tr, "Download total", &util::format_bytes(obj.download()));
    add_kv(&tr, "Upload now", &util::format_speed(obj.now_up()));
    add_kv(&tr, "Download now", &util::format_speed(obj.now_down()));
    add_kv(&tr, "Started", &obj.start());
    page.add(&tr);

    tv.set_content(Some(&page));
    dialog.set_child(Some(&tv));
    dialog.present(parent);
}

fn add_kv(g: &adw::PreferencesGroup, k: &str, v: &str) {
    let row = adw::ActionRow::new();
    row.set_title(k);
    row.set_subtitle(v);
    row.add_css_class("property");
    g.add(&row);
}

use gtk::gio;
use gtk::subclass::prelude::*;

mod imp_conn {
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct ConnObject {
        pub id: RefCell<String>,
        pub host: RefCell<String>,
        pub destination: RefCell<String>,
        pub source: RefCell<String>,
        pub upload: RefCell<u64>,
        pub download: RefCell<u64>,
        pub now_up: RefCell<u64>,
        pub now_down: RefCell<u64>,
        pub chains: RefCell<String>,
        pub rule: RefCell<String>,
        pub process: RefCell<String>,
        pub network: RefCell<String>,
        pub ty: RefCell<String>,
        pub start: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ConnObject {
        const NAME: &'static str = "ConnObject";
        type Type = super::ConnObject;
    }
    impl ObjectImpl for ConnObject {}
}

glib::wrapper! {
    pub struct ConnObject(ObjectSubclass<imp_conn::ConnObject>);
}

impl ConnObject {
    pub fn from_api(c: &crate::api::ConnectionItem, now_up: u64, now_down: u64) -> Self {
        let o: Self = glib::Object::builder().build();
        o.imp().id.replace(c.id.clone());
        o.imp().host.replace(c.metadata.host.clone());
        o.imp().destination.replace(format!(
            "{}:{}", c.metadata.destination_ip, c.metadata.destination_port
        ));
        o.imp().source.replace(format!(
            "{}:{}", c.metadata.source_ip, c.metadata.source_port
        ));
        o.imp().upload.replace(c.upload);
        o.imp().download.replace(c.download);
        o.imp().now_up.replace(now_up);
        o.imp().now_down.replace(now_down);
        o.imp().chains.replace(c.chains.join(" <- "));
        o.imp().rule.replace(if c.rule_payload.is_empty() {
            c.rule.clone()
        } else {
            format!("{}({})", c.rule, c.rule_payload)
        });
        let proc_name = std::path::Path::new(&c.metadata.process_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        o.imp().process.replace(proc_name);
        o.imp().network.replace(c.metadata.network.clone());
        o.imp().ty.replace(c.metadata.ty.clone());
        o.imp().start.replace(c.start.clone());
        o
    }
    pub fn id(&self) -> String { self.imp().id.borrow().clone() }
    pub fn host(&self) -> String { self.imp().host.borrow().clone() }
    pub fn destination(&self) -> String { self.imp().destination.borrow().clone() }
    pub fn source(&self) -> String { self.imp().source.borrow().clone() }
    pub fn upload(&self) -> u64 { *self.imp().upload.borrow() }
    pub fn download(&self) -> u64 { *self.imp().download.borrow() }
    pub fn now_up(&self) -> u64 { *self.imp().now_up.borrow() }
    pub fn now_down(&self) -> u64 { *self.imp().now_down.borrow() }
    pub fn chains(&self) -> String { self.imp().chains.borrow().clone() }
    pub fn rule(&self) -> String { self.imp().rule.borrow().clone() }
    pub fn process(&self) -> String { self.imp().process.borrow().clone() }
    pub fn network(&self) -> String { self.imp().network.borrow().clone() }
    pub fn ty(&self) -> String { self.imp().ty.borrow().clone() }
    pub fn start(&self) -> String { self.imp().start.borrow().clone() }
}
