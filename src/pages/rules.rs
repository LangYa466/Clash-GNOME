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

    let title = gtk::Label::new(Some("Rules"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    toolbar.append(&title);

    let search = gtk::SearchEntry::builder()
        .placeholder_text("Filter rules…")
        .hexpand(true)
        .build();
    toolbar.append(&search);

    let refresh_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.add_css_class("flat");
    refresh_btn.set_tooltip_text(Some("Refresh"));
    toolbar.append(&refresh_btn);

    let count_lbl = gtk::Label::new(Some("0 rules"));
    count_lbl.add_css_class("dim-label");
    count_lbl.add_css_class("caption");
    count_lbl.set_margin_start(20);
    count_lbl.set_margin_end(20);
    count_lbl.set_xalign(0.0);
    root.append(&toolbar);
    root.append(&count_lbl);

    let store = gio::ListStore::new::<RuleObject>();
    let filter = gtk::CustomFilter::new({
        let search = search.clone();
        move |obj| {
            let q = search.text().to_lowercase();
            if q.is_empty() { return true; }
            let rule = obj.downcast_ref::<RuleObject>().unwrap();
            rule.ty().to_lowercase().contains(&q)
                || rule.payload().to_lowercase().contains(&q)
                || rule.proxy().to_lowercase().contains(&q)
        }
    });
    let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

    let selection = gtk::NoSelection::new(Some(filter_model.clone()));

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_top(10);
        row.set_margin_bottom(10);
        row.set_margin_start(20);
        row.set_margin_end(20);

        let type_lbl = gtk::Label::new(None);
        type_lbl.add_css_class("rule-type-badge");
        type_lbl.set_width_request(120);
        type_lbl.set_xalign(0.5);

        let payload_lbl = gtk::Label::new(None);
        payload_lbl.set_xalign(0.0);
        payload_lbl.set_hexpand(true);
        payload_lbl.set_ellipsize(gtk::pango::EllipsizeMode::Middle);

        let proxy_lbl = gtk::Label::new(None);
        proxy_lbl.add_css_class("dim-label");
        proxy_lbl.set_xalign(1.0);

        row.append(&type_lbl);
        row.append(&payload_lbl);
        row.append(&proxy_lbl);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<RuleObject>().unwrap();
        let row = item.child().and_downcast::<gtk::Box>().unwrap();
        let mut child = row.first_child();
        if let Some(t) = child.clone() {
            let t = t.downcast::<gtk::Label>().unwrap();
            t.set_text(&obj.ty());
            child = t.next_sibling();
        }
        if let Some(p) = child.clone() {
            let p = p.downcast::<gtk::Label>().unwrap();
            p.set_text(&obj.payload());
            child = p.next_sibling();
        }
        if let Some(px) = child {
            let px = px.downcast::<gtk::Label>().unwrap();
            px.set_text(&obj.proxy());
        }
    });

    let listview = gtk::ListView::new(Some(selection), Some(factory));
    listview.add_css_class("rule-list");
    listview.set_show_separators(true);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&listview)
        .build();

    let empty = adw::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title("No rules loaded")
        .description("Start the mihomo core and pick an active subscription.")
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&empty, Some("empty"));
    root.append(&stack);

    let count_lbl_c = count_lbl.clone();
    let filter_c = filter.clone();
    search.connect_search_changed(move |_| {
        filter_c.changed(gtk::FilterChange::Different);
        // update count
    });

    let count_update = {
        let count_lbl = count_lbl_c.clone();
        let filter_model = filter_model.clone();
        Rc::new(move || {
            count_lbl.set_text(&format!("{} rules", filter_model.n_items()));
        })
    };
    {
        let cu = count_update.clone();
        filter_model.connect_items_changed(move |_, _, _, _| cu());
    }

    let refresh = {
        let core = state.core.clone();
        let store = store.clone();
        let stack = stack.clone();
        Rc::new(move || {
            let core = core.clone();
            let store = store.clone();
            let stack = stack.clone();
            util::spawn(async move {
                let api = core.api();
                api.rules().await
            }, move |res| {
                match res {
                    Ok(r) => {
                        store.remove_all();
                        for item in r.rules {
                            store.append(&RuleObject::new(&item.ty, &item.payload, &item.proxy));
                        }
                        stack.set_visible_child_name("list");
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
        let r = refresh.clone();
        glib::timeout_add_seconds_local(6, move || {
            r();
            glib::ControlFlow::Continue
        });
        refresh();
    }

    root.upcast()
}

use gtk::gio;
use gtk::subclass::prelude::*;

mod imp_rule {
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct RuleObject {
        pub ty: RefCell<String>,
        pub payload: RefCell<String>,
        pub proxy: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RuleObject {
        const NAME: &'static str = "RuleObject";
        type Type = super::RuleObject;
    }
    impl ObjectImpl for RuleObject {}
}

glib::wrapper! {
    pub struct RuleObject(ObjectSubclass<imp_rule::RuleObject>);
}

impl RuleObject {
    pub fn new(ty: &str, payload: &str, proxy: &str) -> Self {
        let o: Self = glib::Object::builder().build();
        o.imp().ty.replace(ty.to_string());
        o.imp().payload.replace(payload.to_string());
        o.imp().proxy.replace(proxy.to_string());
        o
    }
    pub fn ty(&self) -> String { self.imp().ty.borrow().clone() }
    pub fn payload(&self) -> String { self.imp().payload.borrow().clone() }
    pub fn proxy(&self) -> String { self.imp().proxy.borrow().clone() }
}
