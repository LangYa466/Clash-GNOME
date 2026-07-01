mod api;
mod application;
mod autostart;
mod config;
mod core_manager;
mod mihomo_config;
mod pages;
mod subscription;
mod util;
mod window;

use gtk::prelude::*;

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    util::init_runtime();
    let app = application::ClashGnomeApp::new();
    app.run()
}
