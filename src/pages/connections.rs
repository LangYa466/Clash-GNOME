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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StateFilter { Active, Closed, All }

const MAX_CLOSED: usize = 500;

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

    let state_dd = gtk::DropDown::from_strings(&["Active", "Closed", "All"]);
    state_dd.set_tooltip_text(Some("Connection state"));
    toolbar.append(&state_dd);

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
    let state_filter = Rc::new(std::cell::Cell::new(StateFilter::Active));
    let search_text = Rc::new(RefCell::new(String::new()));
    let closed_conns: Rc<RefCell<Vec<ConnRecord>>> = Rc::new(RefCell::new(Vec::new()));

    let filter = gtk::CustomFilter::new({
        let net_filter = net_filter.clone();
        let search_text = search_text.clone();
        move |obj| {
            let c = obj.downcast_ref::<ConnObject>().unwrap();
            match net_filter.get() {
                NetworkFilter::Tcp if !c.network().eq_ignore_ascii_case("tcp") => return false,
                NetworkFilter::Udp if !c.network().eq_ignore_ascii_case("udp") => return false,
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
                || c.inbound_type().to_lowercase().contains(&q)
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
        let icon = gtk::Image::from_icon_name("application-x-executable-symbolic");
        icon.set_pixel_size(20);
        icon.set_valign(gtk::Align::Center);
        let proto_lbl = gtk::Label::new(None);
        proto_lbl.add_css_class("proto-badge");
        proto_lbl.set_width_request(48);
        let inbound_lbl = gtk::Label::new(None);
        inbound_lbl.add_css_class("proto-badge");
        inbound_lbl.add_css_class("inbound-badge");
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
        l1.append(&icon);
        l1.append(&proto_lbl);
        l1.append(&inbound_lbl);
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
        let icon = l1.first_child().and_then(|c| c.downcast::<gtk::Image>().ok()).unwrap();
        let proto = icon.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let inbound = proto.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let host = inbound.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let now_up = host.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let now_down = now_up.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let close_btn = now_down.next_sibling().and_then(|c| c.downcast::<gtk::Button>().ok()).unwrap();
        let l2 = l1.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();

        // Icon
        let proc_name = obj.process();
        match icon_for_process(&proc_name, &obj.process_path()) {
            Some(g) => icon.set_from_gicon(&g),
            None => icon.set_icon_name(Some("application-x-executable-symbolic")),
        }

        let net = obj.network().to_uppercase();
        proto.set_text(&net);
        for c in ["proto-tcp", "proto-udp", "proto-other"] { proto.remove_css_class(c); }
        proto.add_css_class(match net.as_str() {
            "TCP" => "proto-tcp",
            "UDP" => "proto-udp",
            _ => "proto-other",
        });

        let inbound_text = obj.inbound_type();
        if inbound_text.is_empty() {
            inbound.set_visible(false);
        } else {
            inbound.set_visible(true);
            inbound.set_text(&inbound_text.to_uppercase());
        }

        let display = if obj.host().is_empty() { obj.destination() } else { obj.host() };
        host.set_text(&display);
        now_up.set_text(&format!("up {}", util::format_speed(obj.now_up())));
        now_down.set_text(&format!("dn {}", util::format_speed(obj.now_down())));

        // Close button only for active
        close_btn.set_visible(!obj.is_closed());

        // Row styling for closed
        {
        let c = "conn-closed"; row.remove_css_class(c); }
        if obj.is_closed() { row.add_css_class("conn-closed"); }

        let mut meta = String::new();
        if obj.is_closed() { meta.push_str(&format!("{} | ", obj.closed_ago())); }
        if !obj.chains().is_empty() { meta.push_str(&format!("Chain: {} | ", obj.chains())); }
        if !obj.rule().is_empty() { meta.push_str(&format!("Rule: {} | ", obj.rule())); }
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

    // Speed tracker across polls: id -> (last_upload, last_download, last_time, last_now_up, last_now_down, last_item)
    #[allow(clippy::type_complexity)]
    let prev_snapshot: Rc<RefCell<HashMap<String, (u64, u64, Instant, u64, u64, crate::api::ConnectionItem)>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let refresh = {
        let core = state.core.clone();
        let store = store.clone();
        let stack = stack.clone();
        let summary = summary.clone();
        let sort_key = sort_key.clone();
        let state_filter = state_filter.clone();
        let closed_conns = closed_conns.clone();
        let prev_snapshot = prev_snapshot.clone();
        Rc::new(move || {
            let core = core.clone();
            let store = store.clone();
            let stack = stack.clone();
            let summary = summary.clone();
            let sort_key = sort_key.clone();
            let state_filter = state_filter.clone();
            let closed_conns = closed_conns.clone();
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
                                Some((pu, pd, pt, _, _, _)) => {
                                    let dt = now_ts.duration_since(*pt).as_secs_f64().max(0.001);
                                    let du = c.upload.saturating_sub(*pu) as f64 / dt;
                                    let dd = c.download.saturating_sub(*pd) as f64 / dt;
                                    (du.round() as u64, dd.round() as u64)
                                }
                                None => (0, 0),
                            };
                            new_snap.insert(c.id.clone(), (c.upload, c.download, now_ts, nup, ndown, c.clone()));
                            enriched.push((c.clone(), nup, ndown));
                        }

                        // Detect newly closed connections
                        let mut closed = closed_conns.borrow_mut();
                        for (id, (pu, pd, _, nup, ndown, item)) in prev.iter() {
                            if !new_snap.contains_key(id) {
                                let mut item = item.clone();
                                item.upload = *pu;
                                item.download = *pd;
                                closed.push(ConnRecord {
                                    item,
                                    now_up: *nup,
                                    now_down: *ndown,
                                    closed_at: Instant::now(),
                                });
                            }
                        }
                        if closed.len() > MAX_CLOSED {
                            let excess = closed.len() - MAX_CLOSED;
                            closed.drain(0..excess);
                        }
                        *prev = new_snap;

                        // Sort active
                        match sort_key.get() {
                            SortKey::Time => enriched.sort_by(|a, b| b.0.start.cmp(&a.0.start)),
                            SortKey::NowUp => enriched.sort_by_key(|e| std::cmp::Reverse(e.1)),
                            SortKey::NowDown => enriched.sort_by_key(|e| std::cmp::Reverse(e.2)),
                            SortKey::AllUp => enriched.sort_by_key(|e| std::cmp::Reverse(e.0.upload)),
                            SortKey::AllDown => enriched.sort_by_key(|e| std::cmp::Reverse(e.0.download)),
                        }
                        // Sort closed (most recent first)
                        let mut closed_sorted: Vec<&ConnRecord> = closed.iter().collect();
                        match sort_key.get() {
                            SortKey::Time => closed_sorted.sort_by_key(|r| std::cmp::Reverse(r.closed_at)),
                            SortKey::NowUp => closed_sorted.sort_by_key(|r| std::cmp::Reverse(r.now_up)),
                            SortKey::NowDown => closed_sorted.sort_by_key(|r| std::cmp::Reverse(r.now_down)),
                            SortKey::AllUp => closed_sorted.sort_by_key(|r| std::cmp::Reverse(r.item.upload)),
                            SortKey::AllDown => closed_sorted.sort_by_key(|r| std::cmp::Reverse(r.item.download)),
                        }

                        store.remove_all();
                        let sf = state_filter.get();
                        let mut visible_count = 0usize;
                        if matches!(sf, StateFilter::Active | StateFilter::All) {
                            for (c, nup, ndown) in &enriched {
                                store.append(&ConnObject::from_api(c, *nup, *ndown));
                                visible_count += 1;
                            }
                        }
                        if matches!(sf, StateFilter::Closed | StateFilter::All) {
                            for rec in &closed_sorted {
                                store.append(&ConnObject::from_closed(rec));
                                visible_count += 1;
                            }
                        }
                        summary.set_text(&format!(
                            "{} active | {} closed | Up {} | Down {}",
                            enriched.len(),
                            closed.len(),
                            util::format_bytes(r.upload_total),
                            util::format_bytes(r.download_total),
                        ));
                        if visible_count == 0 {
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
        let state_filter = state_filter.clone();
        let r = refresh.clone();
        state_dd.connect_selected_notify(move |dd| {
            state_filter.set(match dd.selected() {
                1 => StateFilter::Closed,
                2 => StateFilter::All,
                _ => StateFilter::Active,
            });
            r();
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
    add_kv(&group, "Inbound Type", &obj.inbound_type());
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

thread_local! {
    static ICON_CACHE: RefCell<Option<HashMap<String, gio::Icon>>> = const { RefCell::new(None) };
    static ICON_MISS: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
}

fn build_icon_cache() -> HashMap<String, gio::Icon> {
    let mut map = HashMap::new();
    for info in gio::AppInfo::all() {
        let exec = info.executable();
        let name = exec.file_name().and_then(|s| s.to_str()).map(|s| s.to_string());
        if let (Some(name), Some(icon)) = (name, info.icon()) {
            map.entry(name.to_lowercase()).or_insert(icon);
        }
    }
    map
}

fn icon_for_process(name: &str, path: &str) -> Option<gio::Icon> {
    if name.is_empty() { return None; }
    ICON_CACHE.with(|c| {
        if c.borrow().is_none() { *c.borrow_mut() = Some(build_icon_cache()); }
    });
    let key = name.to_lowercase();
    let hit = ICON_CACHE.with(|c| c.borrow().as_ref().and_then(|m| m.get(&key).cloned()));
    if hit.is_some() { return hit; }

    // Try path basename without extension (e.g., /snap/foo/bin/foo)
    if !path.is_empty() {
        let stem = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).map(|s| s.to_lowercase());
        if let Some(s) = stem {
            let hit = ICON_CACHE.with(|c| c.borrow().as_ref().and_then(|m| m.get(&s).cloned()));
            if hit.is_some() { return hit; }
        }
    }
    ICON_MISS.with(|m| { m.borrow_mut().insert(key); });
    None
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
        pub process_path: RefCell<String>,
        pub network: RefCell<String>,
        pub inbound_type: RefCell<String>,
        pub start: RefCell<String>,
        pub is_closed: RefCell<bool>,
        pub closed_ago: RefCell<String>,
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
        o.imp().process_path.replace(c.metadata.process_path.clone());
        o.imp().network.replace(c.metadata.network.clone());
        o.imp().inbound_type.replace(c.metadata.ty.clone());
        o.imp().start.replace(c.start.clone());
        o.imp().is_closed.replace(false);
        o
    }
    pub fn from_closed(rec: &ConnRecord) -> Self {
        let o = Self::from_api(&rec.item, rec.now_up, rec.now_down);
        o.imp().is_closed.replace(true);
        let secs = rec.closed_at.elapsed().as_secs();
        o.imp().closed_ago.replace(format!("closed {}s ago", secs));
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
    pub fn process_path(&self) -> String { self.imp().process_path.borrow().clone() }
    pub fn network(&self) -> String { self.imp().network.borrow().clone() }
    pub fn inbound_type(&self) -> String { self.imp().inbound_type.borrow().clone() }
    pub fn start(&self) -> String { self.imp().start.borrow().clone() }
    pub fn is_closed(&self) -> bool { *self.imp().is_closed.borrow() }
    pub fn closed_ago(&self) -> String { self.imp().closed_ago.borrow().clone() }
}

pub struct ConnRecord {
    pub item: crate::api::ConnectionItem,
    pub now_up: u64,
    pub now_down: u64,
    pub closed_at: Instant,
}
