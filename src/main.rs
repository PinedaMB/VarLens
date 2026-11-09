mod core;
mod i18n;
mod ui;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use ui::window::VarDiffWindow;

const APP_ID: &str = "io.github.PinedaMB.VarLens";

fn main() -> glib::ExitCode {
    // If GSK_RENDERER is not explicitly specified, prefer cairo or gl to avoid Vulkan swapchain OOM crashes on Linux
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "gl");
    }

    if let Err(e) = gtk4::init() {
        eprintln!("Error al inicializar GTK4: {}", e);
        return glib::ExitCode::FAILURE;
    }
    i18n::init();
    sourceview5::init();
    ui::diff_editor::VarLensDiffView::ensure_type();

    // Register custom style schemes
    let cache_dir = std::env::temp_dir().join("varlens/styles");
    let _ = std::fs::create_dir_all(&cache_dir);
    let style_dark = cache_dir.join("varlens-dark.xml");
    let _ = std::fs::write(&style_dark, include_str!("../data/styles/varlens-dark.xml"));
    let style_light = cache_dir.join("varlens-light.xml");
    let _ = std::fs::write(&style_light, include_str!("../data/styles/varlens-light.xml"));

    let scheme_mgr = sourceview5::StyleSchemeManager::default();
    scheme_mgr.append_search_path("data/styles");
    if let Some(path_str) = cache_dir.to_str() {
        scheme_mgr.append_search_path(path_str);
    }

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
                    .application_name("VarLens")
                    .application_icon("io.github.PinedaMB.VarLens")
                    .developer_name("VarLens Team")
                    .version("1.0.0")
                    .website("https://github.com/PinedaMB/VarLens")
                    .license_type(gtk4::License::MitX11)
                    .comments(tr!(
                        "Herramienta de desarrollo nativa para Linux para formatear y validar JSON, explorar árboles de datos, convertir salidas de var_dump() y print_r() de PHP y comparar diffs visualmente.",
                        "Native Linux developer tool for JSON formatting & validation, interactive tree exploration, PHP var_dump() and print_r() conversion, and visual diff inspection."
                    ))
                    .developers(vec!["VarLens Developers"])
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
