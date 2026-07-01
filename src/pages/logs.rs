use crate::application::AppState;
use crate::core_manager::CoreState;
use crate::util;
use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn build(state: Arc<AppState>) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    toolbar.set_margin_top(16);
    toolbar.set_margin_start(20);
    toolbar.set_margin_end(20);
    toolbar.set_margin_bottom(10);
    let title = gtk::Label::new(Some("Logs"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    toolbar.append(&title);

    let level_dd = gtk::DropDown::from_strings(&["info", "warning", "error", "debug", "silent"]);
    level_dd.set_selected(0);
    toolbar.append(&level_dd);

    let clear_btn = gtk::Button::with_label("Clear");
    clear_btn.add_css_class("flat");
    toolbar.append(&clear_btn);

    let pause_btn = gtk::ToggleButton::builder()
        .icon_name("media-playback-pause-symbolic")
        .tooltip_text("Pause / resume auto-scroll")
        .css_classes(["flat"])
        .build();
    toolbar.append(&pause_btn);

    root.append(&toolbar);

    let text_view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(10)
        .bottom_margin(10)
        .left_margin(14)
        .right_margin(14)
        .build();
    text_view.add_css_class("log-view");
    let buffer = text_view.buffer();

    // Tags for level coloring
    let tag_table = buffer.tag_table();
    for (name, color) in &[
        ("log-info", "#8ab4f8"),
        ("log-warning", "#fbbc04"),
        ("log-error", "#f28b82"),
        ("log-debug", "#a1a1aa"),
        ("log-kernel", "#c5a2f8"),
    ] {
        let tag = gtk::TextTag::builder().name(*name).foreground(*color).build();
        tag_table.add(&tag);
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&text_view)
        .build();
    scrolled.add_css_class("log-scroll");
    root.append(&scrolled);

    // Kernel stdout/stderr consumer
    {
        let buffer = buffer.clone();
        let core = state.core.clone();
        let scrolled_c = scrolled.clone();
        let pause_c = pause_btn.clone();
        glib::MainContext::default().spawn_local(async move {
            let mut rx = match core.take_log_rx().await {
                Some(rx) => rx,
                None => return,
            };
            while let Some(line) = rx.recv().await {
                let mut end = buffer.end_iter();
                buffer.insert_with_tags_by_name(&mut end, &format!("{line}\n"), &["log-kernel"]);
                trim_buffer(&buffer);
                if !pause_c.is_active() { autoscroll(&scrolled_c); }
            }
        });
    }

    // /logs stream from API (only when running); rebuild subscription when level changes
    let stream_handle: Rc<RefCell<Option<tokio::sync::mpsc::Sender<()>>>> = Rc::new(RefCell::new(None));

    let restart_stream = {
        let core = state.core.clone();
        let buffer = buffer.clone();
        let scrolled = scrolled.clone();
        let pause = pause_btn.clone();
        let stream_handle = stream_handle.clone();
        let level_dd = level_dd.clone();
        Rc::new(move || {
            if let Some(tx) = stream_handle.borrow_mut().take() {
                let _ = tx.try_send(());
            }
            let core = core.clone();
            let buffer = buffer.clone();
            let scrolled = scrolled.clone();
            let pause = pause.clone();
            let stream_handle = stream_handle.clone();
            let level = match level_dd.selected() {
                1 => "warning",
                2 => "error",
                3 => "debug",
                4 => "silent",
                _ => "info",
            };
            let level_str = level.to_string();
            let core_state = core.clone();
            util::spawn(async move { core_state.state().await }, move |s| {
                if s != CoreState::Running { return; }
                let api = Arc::new(core.api());
                let (mut rx, cancel_tx) = api.stream_logs(&level_str);
                *stream_handle.borrow_mut() = Some(cancel_tx);
                let buffer_c = buffer.clone();
                let scrolled_c = scrolled.clone();
                let pause_c = pause.clone();
                glib::MainContext::default().spawn_local(async move {
                    while let Some(ev) = rx.recv().await {
                        let tag = match ev.level.as_str() {
                            "warning" => "log-warning",
                            "error" => "log-error",
                            "debug" => "log-debug",
                            _ => "log-info",
                        };
                        let mut end = buffer_c.end_iter();
                        let ts = chrono::Local::now().format("%H:%M:%S");
                        buffer_c.insert_with_tags_by_name(
                            &mut end,
                            &format!("[{ts}] [{}] {}\n", ev.level.to_uppercase(), ev.payload),
                            &[tag],
                        );
                        trim_buffer(&buffer_c);
                        if !pause_c.is_active() { autoscroll(&scrolled_c); }
                    }
                });
            });
        })
    };

    {
        let restart = restart_stream.clone();
        level_dd.connect_selected_notify(move |_| restart());
    }
    // Restart when core state flips
    {
        let core = state.core.clone();
        let restart = restart_stream.clone();
        let last = Rc::new(RefCell::new(CoreState::Stopped));
        glib::timeout_add_seconds_local(1, move || {
            let core = core.clone();
            let restart = restart.clone();
            let last = last.clone();
            util::spawn(async move { core.state().await }, move |s| {
                if *last.borrow() != s {
                    *last.borrow_mut() = s;
                    if s == CoreState::Running {
                        restart();
                    }
                }
            });
            glib::ControlFlow::Continue
        });
    }
    {
        let buffer = buffer.clone();
        clear_btn.connect_clicked(move |_| {
            buffer.set_text("");
        });
    }

    root.upcast()
}

fn autoscroll(scrolled: &gtk::ScrolledWindow) {
    let adj = scrolled.vadjustment();
    adj.set_value(adj.upper() - adj.page_size());
}

fn trim_buffer(buffer: &gtk::TextBuffer) {
    // Keep at most ~5000 lines
    const MAX_LINES: i32 = 5000;
    let lines = buffer.line_count();
    if lines > MAX_LINES {
        let mut start = buffer.start_iter();
        let mut cut_at = buffer.iter_at_line(lines - MAX_LINES).unwrap_or(buffer.start_iter());
        buffer.delete(&mut start, &mut cut_at);
    }
}
