mod core;
mod ui;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use ui::window::MainWindow;

const APP_ID: &str = "io.github.varlens.App";

fn main() -> glib::ExitCode {
    // Initialize GTK and SourceView5
    if let Err(e) = gtk4::init() {
        eprintln!("Error al inicializar GTK4: {}", e);
        return glib::ExitCode::FAILURE;
    }
    sourceview5::init();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            icon_theme.add_search_path("data/icons");
        }
    });

    app.connect_activate(|app| {
        let main_window = MainWindow::build(app);
        main_window.present();
    });

    app.run()
}
