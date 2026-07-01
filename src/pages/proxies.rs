use crate::application::AppState;
use crate::util;
use adw::prelude::*;
use gtk::glib;
use std::rc::Rc;
use std::sync::Arc;

const TEST_URL: &str = "https://www.gstatic.com/generate_204";
const TEST_TIMEOUT_MS: u32 = 3000;

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toolbar.set_margin_top(16);
    toolbar.set_margin_start(20);
    toolbar.set_margin_end(20);
    toolbar.set_margin_bottom(6);

    let title = gtk::Label::new(Some("Proxy Groups"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    toolbar.append(&title);

    let refresh_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.set_tooltip_text(Some("Refresh proxies"));
    refresh_btn.add_css_class("flat");
    toolbar.append(&refresh_btn);

    let test_all_btn = gtk::Button::builder()
        .label("Test All")
        .css_classes(["pill"])
        .build();
    toolbar.append(&test_all_btn);

    content.append(&toolbar);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(1100)
        .tightening_threshold(900)
        .build();
    let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
    groups_box.set_margin_top(6);
    groups_box.set_margin_bottom(24);
    groups_box.set_margin_start(20);
    groups_box.set_margin_end(20);
    clamp.set_child(Some(&groups_box));
    scrolled.set_child(Some(&clamp));
    content.append(&scrolled);

    stack.add_named(&content, Some("main"));

    let empty = adw::StatusPage::builder()
        .icon_name("network-offline-symbolic")
        .title("Core is not running")
        .description("Start the mihomo core to see proxy groups.")
        .build();
    stack.add_named(&empty, Some("empty"));

    let groups_box_rc = Rc::new(groups_box);
    let refresh_state = state.clone();
    let groups_box_c = groups_box_rc.clone();
    let stack_c = stack.clone();
    let refresh = move || {
        let core = refresh_state.core.clone();
        let groups_box = groups_box_c.clone();
        let stack = stack_c.clone();
        let state = refresh_state.clone();
        util::spawn(async move {
            let api = core.api();
            api.proxies().await
        }, move |res| {
            match res {
                Ok(resp) => {
                    stack.set_visible_child_name("main");
                    render_groups(&groups_box, &resp, state.clone());
                }
                Err(_) => {
                    stack.set_visible_child_name("empty");
                }
            }
        });
    };

    {
        let refresh = refresh.clone();
        refresh_btn.connect_clicked(move |_| refresh());
    }
    {
        let core = state.core.clone();
        let refresh = refresh.clone();
        test_all_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let core = core.clone();
            let refresh_after = refresh.clone();
            let b_c = b.clone();
            util::spawn(async move {
                let api = core.api();
                if let Ok(p) = api.proxies().await {
                    for (name, item) in p.proxies.iter() {
                        if matches!(item.ty.as_str(), "Selector" | "URLTest" | "Fallback" | "LoadBalance" | "Relay") {
                            let _ = api.test_group_delay(name, TEST_URL, TEST_TIMEOUT_MS).await;
                        }
                    }
                }
            }, move |_| {
                b_c.set_sensitive(true);
                refresh_after();
            });
        });
    }

    // Auto-refresh on show / periodic
    {
        refresh();
        let refresh_tick = refresh.clone();
        glib::timeout_add_seconds_local(5, move || {
            refresh_tick();
            glib::ControlFlow::Continue
        });
    }

    stack.upcast()
}

fn render_groups(container: &gtk::Box, resp: &crate::api::ProxiesResponse, state: Arc<AppState>) {
    while let Some(c) = container.first_child() {
        container.remove(&c);
    }
    let mut groups: Vec<(&String, &crate::api::ProxyItem)> = resp
        .proxies
        .iter()
        .filter(|(_, v)| matches!(v.ty.as_str(), "Selector" | "URLTest" | "Fallback" | "LoadBalance" | "Relay"))
        .collect();
    groups.sort_by(|a, b| a.0.cmp(b.0));

    if groups.is_empty() {
        let sp = adw::StatusPage::builder()
            .icon_name("dialog-information-symbolic")
            .title("No proxy groups")
            .description("Your active subscription doesn't define proxy groups.")
            .build();
        container.append(&sp);
        return;
    }

    for (name, item) in groups {
        container.append(&group_card(name, item, resp, state.clone()));
    }
}

fn group_card(
    name: &str,
    item: &crate::api::ProxyItem,
    all_proxies: &crate::api::ProxiesResponse,
    state: Arc<AppState>,
) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("card");
    card.add_css_class("proxy-group");
    card.set_margin_top(2);

    // header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.set_margin_top(14);
    header.set_margin_start(16);
    header.set_margin_end(16);

    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let name_lbl = gtk::Label::new(Some(name));
    name_lbl.add_css_class("title-3");
    name_lbl.set_xalign(0.0);
    let subtitle = format!(
        "{} · {} nodes · Current: {}",
        item.ty,
        item.all.as_ref().map(|v| v.len()).unwrap_or(0),
        item.now.as_deref().unwrap_or("-")
    );
    let sub_lbl = gtk::Label::new(Some(&subtitle));
    sub_lbl.add_css_class("dim-label");
    sub_lbl.add_css_class("caption");
    sub_lbl.set_xalign(0.0);
    title_box.append(&name_lbl);
    title_box.append(&sub_lbl);
    title_box.set_hexpand(true);
    header.append(&title_box);

    let test_btn = gtk::Button::from_icon_name("emoji-recent-symbolic");
    test_btn.set_tooltip_text(Some("Test latency for all members"));
    test_btn.add_css_class("flat");
    let group_name = name.to_string();
    {
        let core = state.core.clone();
        test_btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            let core = core.clone();
            let gname = group_name.clone();
            let b_c = b.clone();
            util::spawn(async move {
                let api = core.api();
                api.test_group_delay(&gname, TEST_URL, TEST_TIMEOUT_MS).await.ok();
            }, move |_| {
                b_c.set_sensitive(true);
            });
        });
    }
    header.append(&test_btn);
    card.append(&header);

    // Node grid
    let flow = gtk::FlowBox::builder()
        .homogeneous(true)
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(2)
        .max_children_per_line(4)
        .row_spacing(8)
        .column_spacing(8)
        .margin_top(4)
        .margin_bottom(14)
        .margin_start(16)
        .margin_end(16)
        .build();

    let is_selector = item.ty == "Selector";
    let current = item.now.clone().unwrap_or_default();
    let members = item.all.clone().unwrap_or_default();

    for member in members {
        let btn = gtk::Button::new();
        btn.add_css_class("proxy-node");
        if member == current {
            btn.add_css_class("proxy-node-active");
        }
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.set_margin_top(8);
        hbox.set_margin_bottom(8);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        let dot = gtk::Label::new(Some("●"));
        let delay = last_delay(all_proxies, &member);
        let (delay_text, delay_class) = format_delay(delay);
        dot.add_css_class(delay_class);
        let label = gtk::Label::new(Some(&member));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_hexpand(true);
        let delay_lbl = gtk::Label::new(Some(&delay_text));
        delay_lbl.add_css_class("dim-label");
        delay_lbl.add_css_class("caption");
        hbox.append(&dot);
        hbox.append(&label);
        hbox.append(&delay_lbl);
        btn.set_child(Some(&hbox));

        if is_selector {
            let group_name = name.to_string();
            let member_c = member.clone();
            let core = state.core.clone();
            btn.connect_clicked(move |_| {
                let core = core.clone();
                let gname = group_name.clone();
                let m = member_c.clone();
                util::detach(async move {
                    let api = core.api();
                    let _ = api.select_proxy(&gname, &m).await;
                });
            });
        } else {
            btn.set_sensitive(false);
        }
        flow.insert(&btn, -1);
    }
    card.append(&flow);

    card.upcast()
}

fn last_delay(all_proxies: &crate::api::ProxiesResponse, name: &str) -> Option<u32> {
    let item = all_proxies.proxies.get(name)?;
    item.history.last().map(|h| h.delay).filter(|d| *d > 0)
}

fn format_delay(d: Option<u32>) -> (String, &'static str) {
    match d {
        None => ("—".to_string(), "delay-unknown"),
        Some(v) if v < 200 => (format!("{} ms", v), "delay-good"),
        Some(v) if v < 500 => (format!("{} ms", v), "delay-mid"),
        Some(v) => (format!("{} ms", v), "delay-bad"),
    }
}

