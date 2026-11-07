mod core;
mod ui;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use ui::window::VarDiffWindow;

const APP_ID: &str = "io.github.vardiff.App";

fn main() -> glib::ExitCode {
    if let Err(e) = gtk4::init() {
        eprintln!("Error al inicializar GTK4: {}", e);
        return glib::ExitCode::FAILURE;
    }
    sourceview5::init();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|app| {
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            icon_theme.add_search_path("data/icons");

            // Load custom application CSS
            let provider = gtk4::CssProvider::new();
            provider.load_from_string(include_str!("ui/style.css"));
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        // Action: app.about
        let act_about = gio::SimpleAction::new("about", None);
        let app_weak = app.downgrade();
        act_about.connect_activate(move |_, _| {
            if let Some(app) = app_weak.upgrade() {
                let active_win = app.active_window();
                let about = adw::AboutDialog::builder()
                    .application_name("VarDiff")
                    .application_icon("io.github.varlens.App")
                    .developer_name("VarDiff Team")
                    .version("1.2.0")
                    .website("https://github.com/varlens/vardiff")
                    .license_type(gtk4::License::MitX11)
                    .comments("Herramienta de desarrollo nativa para Linux para formatear y validar JSON, convertir salidas de var_dump() de PHP y comparar diffs visualmente.")
                    .developers(vec!["VarDiff Developers"])
                    .build();
                about.present(active_win.as_ref());
            }
        });
        app.add_action(&act_about);
    });

    app.connect_activate(|app| {
        let window = VarDiffWindow::new(app);
        window.present();
    });

    app.run()
}
