use crate::application::AppState;
use crate::util;
use adw::prelude::*;
use gtk::glib;
use std::rc::Rc;
use std::sync::Arc;

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
        .placeholder_text("Filter host…")
        .hexpand(true)
        .build();
    toolbar.append(&search);

    let close_all_btn = gtk::Button::builder()
        .label("Close All")
        .css_classes(["destructive-action", "pill"])
        .build();
    toolbar.append(&close_all_btn);

    let refresh_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.add_css_class("flat");
    toolbar.append(&refresh_btn);

    root.append(&toolbar);

    let summary = gtk::Label::new(Some("0 active · Up 0 B · Down 0 B"));
    summary.add_css_class("dim-label");
    summary.add_css_class("caption");
    summary.set_xalign(0.0);
    summary.set_margin_start(20);
    summary.set_margin_end(20);
    root.append(&summary);

    let store = gio::ListStore::new::<ConnObject>();
    let filter = gtk::CustomFilter::new({
        let search = search.clone();
        move |obj| {
            let q = search.text().to_lowercase();
            if q.is_empty() { return true; }
            let c = obj.downcast_ref::<ConnObject>().unwrap();
            c.host().to_lowercase().contains(&q)
                || c.destination().to_lowercase().contains(&q)
                || c.chains().to_lowercase().contains(&q)
                || c.rule().to_lowercase().contains(&q)
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
        let host = gtk::Label::new(None);
        host.set_xalign(0.0);
        host.set_hexpand(true);
        host.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        host.add_css_class("body");
        let updown = gtk::Label::new(None);
        updown.add_css_class("dim-label");
        updown.add_css_class("caption");
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.add_css_class("circular");
        close_btn.set_tooltip_text(Some("Close this connection"));
        l1.append(&host);
        l1.append(&updown);
        l1.append(&close_btn);

        let l2 = gtk::Label::new(None);
        l2.set_xalign(0.0);
        l2.add_css_class("dim-label");
        l2.add_css_class("caption");
        l2.set_ellipsize(gtk::pango::EllipsizeMode::End);

        row.append(&l1);
        row.append(&l2);
        item.set_child(Some(&row));

        // Bind close handler ONCE per setup; use the ListItem's current item at click time.
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
        let host = l1.first_child().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let updown = host.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();
        let l2 = l1.next_sibling().and_then(|c| c.downcast::<gtk::Label>().ok()).unwrap();

        let display = if obj.host().is_empty() { obj.destination() } else { obj.host() };
        host.set_text(&display);
        updown.set_text(&format!("↑ {}  ↓ {}", util::format_bytes(obj.upload()), util::format_bytes(obj.download())));

        let mut meta = String::new();
        if !obj.chains().is_empty() { meta.push_str(&format!("Chain: {} · ", obj.chains())); }
        if !obj.rule().is_empty() { meta.push_str(&format!("Rule: {} · ", obj.rule())); }
        let proc_name = obj.process();
        meta.push_str(&format!("Process: {}", if proc_name.is_empty() { "-" } else { proc_name.as_str() }));
        l2.set_text(&meta);
    });

    let listview = gtk::ListView::new(Some(selection), Some(factory));
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

    let refresh = {
        let core = state.core.clone();
        let store = store.clone();
        let stack = stack.clone();
        let summary = summary.clone();
        Rc::new(move || {
            let core = core.clone();
            let store = store.clone();
            let stack = stack.clone();
            let summary = summary.clone();
            util::spawn(async move {
                let api = core.api();
                api.connections().await
            }, move |res| {
                match res {
                    Ok(r) => {
                        store.remove_all();
                        let mut conns = r.connections.clone();
                        conns.sort_by_key(|c| std::cmp::Reverse(c.download));
                        for c in &conns {
                            store.append(&ConnObject::from_api(c));
                        }
                        summary.set_text(&format!(
                            "{} active · Up {} · Down {}",
                            conns.len(),
                            util::format_bytes(r.upload_total),
                            util::format_bytes(r.download_total),
                        ));
                        if conns.is_empty() {
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
    let search_c = search.clone();
    let filter_c = filter.clone();
    search_c.connect_search_changed(move |_| filter_c.changed(gtk::FilterChange::Different));
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
        glib::timeout_add_seconds_local(2, move || {
            r();
            glib::ControlFlow::Continue
        });
        refresh();
    }

    root.upcast()
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
        pub upload: RefCell<u64>,
        pub download: RefCell<u64>,
        pub chains: RefCell<String>,
        pub rule: RefCell<String>,
        pub process: RefCell<String>,
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
    pub fn from_api(c: &crate::api::ConnectionItem) -> Self {
        let o: Self = glib::Object::builder().build();
        o.imp().id.replace(c.id.clone());
        o.imp().host.replace(c.metadata.host.clone());
        o.imp().destination.replace(format!(
            "{}:{}",
            c.metadata.destination_ip, c.metadata.destination_port
        ));
        o.imp().upload.replace(c.upload);
        o.imp().download.replace(c.download);
        o.imp().chains.replace(c.chains.join(" ← "));
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
        o
    }
    pub fn id(&self) -> String { self.imp().id.borrow().clone() }
    pub fn host(&self) -> String { self.imp().host.borrow().clone() }
    pub fn destination(&self) -> String { self.imp().destination.borrow().clone() }
    pub fn upload(&self) -> u64 { *self.imp().upload.borrow() }
    pub fn download(&self) -> u64 { *self.imp().download.borrow() }
    pub fn chains(&self) -> String { self.imp().chains.borrow().clone() }
    pub fn rule(&self) -> String { self.imp().rule.borrow().clone() }
    pub fn process(&self) -> String { self.imp().process.borrow().clone() }
}
