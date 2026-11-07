use crate::core::diff::{compute_diff, DiffSummary};
use crate::core::json::{count_lines, format_json, validate_json};
use crate::core::vardump::vardump_to_json_string;
use crate::ui::diff_view::DiffView;
use crate::ui::editor::EditorView;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const SAMPLE_VARDUMP: &str = r#"object(App\Models\User)#42 (4) {
  ["id"]=>
  int(101)
  ["name"]=>
  string(14) "Carlos Mendoza"
  ["email"]=>
  string(23) "carlos.mendoza@php.test"
  ["roles"]=>
  array(3) {
    [0]=>
    string(5) "admin"
    [1]=>
    string(9) "developer"
    [2]=>
    string(7) "manager"
  }
  ["settings"]=>
  array(2) {
    ["theme"]=>
    string(4) "dark"
    ["notifications"]=>
    bool(true)
  }
  ["api_key":"App\Models\User":private]=>
  string(32) "9b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e"
}"#;

const SAMPLE_JSON: &str = r#"{
  "project": "VarLens",
  "version": "0.1.0",
  "features": [
    "JSON Prettify & Minify",
    "PHP var_dump to JSON",
    "Side-by-side & Unified Diff"
  ],
  "engine": {
    "language": "Rust",
    "gui": "GTK4 / Libadwaita",
    "diff_algo": "Myers / Patience"
  },
  "active": true
}"#;

const SAMPLE_DIFF_OLD: &str = r#"{
  "name": "VarLens",
  "version": "0.1.0",
  "author": "Antigravity",
  "status": "beta",
  "dependencies": [
    "gtk4",
    "libadwaita"
  ]
}"#;

const SAMPLE_DIFF_NEW: &str = r#"{
  "name": "VarLens",
  "version": "1.0.0",
  "author": "Antigravity Team",
  "status": "stable",
  "dependencies": [
    "gtk4",
    "libadwaita",
    "sourceview5",
    "similar"
  ]
}"#;

const ASYNC_LINE_THRESHOLD: usize = 1000;

enum WorkerResult {
    EditorFormatted { text: String, message: String },
    DiffCalculated(DiffSummary),
    Error(String),
}

pub struct MainWindow {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    view_stack: adw::ViewStack,
    editor_view: EditorView,
    diff_view: DiffView,
    spinner: gtk4::Spinner,
    status_label: gtk4::Label,
    is_busy: Arc<AtomicBool>,
}

impl MainWindow {
    pub fn build(app: &adw::Application) -> Self {
        // Main editors
        let editor_view = EditorView::new(Some("json"));
        let diff_view = DiffView::new();

        // Loading spinner and status indicator
        let spinner = gtk4::Spinner::builder()
            .spinning(false)
            .visible(false)
            .build();

        let status_label = gtk4::Label::builder()
            .label("Listo")
            .css_classes(["dim-label"])
            .build();

        let status_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(6)
            .margin_top(4)
            .build();
        status_box.append(&spinner);
        status_box.append(&status_label);

        // View Stack
        let view_stack = adw::ViewStack::new();

        // 1. Single Editor View Container
        let editor_container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        editor_container.append(editor_view.widget());

        let editor_page = view_stack.add_titled(&editor_container, Some("editor"), "Editor Único");
        editor_page.set_icon_name(Some("text-editor-symbolic"));

        // 2. Diff View Container
        let diff_page = view_stack.add_titled(diff_view.widget(), Some("diff"), "Comparador Diff");
        diff_page.set_icon_name(Some("edit-find-replace-symbolic"));

        // View Switcher in HeaderBar
        let view_switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Wide)
            .build();

        // Quick Action Buttons
        let btn_prettify = gtk4::Button::builder()
            .label("Prettify")
            .tooltip_text("Dar formato legible y validar JSON")
            .css_classes(["suggested-action"])
            .build();

        let btn_minify = gtk4::Button::builder()
            .label("Minify")
            .tooltip_text("Minificar y comprimir JSON")
            .build();

        let btn_validate = gtk4::Button::builder()
            .label("Validar")
            .tooltip_text("Validar sintaxis JSON sin modificar formato")
            .build();

        let btn_vardump = gtk4::Button::builder()
            .label("var_dump ➔ JSON")
            .tooltip_text("Convertir salida cruda de PHP var_dump() a JSON estructurado")
            .build();

        let btn_diff = gtk4::Button::builder()
            .label("Calcular Diff")
            .tooltip_text("Comparar lado a lado y resaltar diferencias")
            .css_classes(["accent"])
            .build();

        let btn_copy = gtk4::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copiar contenido al portapapeles")
            .build();

        let btn_clear = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Limpiar contenido del editor")
            .build();

        // Header bar layout
        let header_bar = adw::HeaderBar::builder()
            .title_widget(&view_switcher)
            .build();

        // Start actions (Left of header)
        header_bar.pack_start(&btn_prettify);
        header_bar.pack_start(&btn_minify);
        header_bar.pack_start(&btn_validate);
        header_bar.pack_start(&btn_vardump);
        header_bar.pack_start(&btn_diff);

        let menu = gio::Menu::new();

        let samples_section = gio::Menu::new();
        samples_section.append(Some("Cargar ejemplo var_dump"), Some("win.sample_vardump"));
        samples_section.append(Some("Cargar ejemplo JSON"), Some("win.sample_json"));
        samples_section.append(Some("Cargar ejemplo Diff"), Some("win.sample_diff"));
        menu.append_section(Some("Ejemplos"), &samples_section);

        let info_section = gio::Menu::new();
        info_section.append(Some("Atajos de teclado"), Some("win.shortcuts"));
        info_section.append(Some("Acerca de VarLens"), Some("win.about"));
        menu.append_section(None, &info_section);

        let menu_button = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .tooltip_text("Menú principal")
            .build();

        // End actions (Right of header)
        header_bar.pack_end(&menu_button);
        header_bar.pack_end(&btn_clear);
        header_bar.pack_end(&btn_copy);

        // Main layout box
        let content_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        content_box.append(&header_bar);
        content_box.append(&view_stack);
        content_box.append(&status_box);

        // Toast overlay for feedback
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&content_box));

        // Application Window
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("VarLens")
            .default_width(1150)
            .default_height(780)
            .content(&toast_overlay)
            .build();

        // Synchronize dark theme with SourceView style schemes
        let style_manager = adw::StyleManager::default();
        let editor_buf = editor_view.buffer().clone();
        let diff_left_buf = diff_view.left_editor().buffer().clone();
        let diff_right_buf = diff_view.right_editor().buffer().clone();

        style_manager.connect_dark_notify(move |sm| {
            let is_dark = sm.is_dark();
            EditorView::update_scheme(&editor_buf, is_dark);
            EditorView::update_scheme(&diff_left_buf, is_dark);
            EditorView::update_scheme(&diff_right_buf, is_dark);
        });

        // Set initial dark theme if active
        if style_manager.is_dark() {
            EditorView::update_scheme(editor_view.buffer(), true);
            EditorView::update_scheme(diff_view.left_editor().buffer(), true);
            EditorView::update_scheme(diff_view.right_editor().buffer(), true);
        }

        let is_busy = Arc::new(AtomicBool::new(false));

        let app_window = Self {
            window,
            toast_overlay,
            view_stack,
            editor_view,
            diff_view,
            spinner,
            status_label,
            is_busy,
        };

        app_window.setup_actions(
            &btn_prettify,
            &btn_minify,
            &btn_validate,
            &btn_vardump,
            &btn_diff,
            &btn_copy,
            &btn_clear,
        );

        let handles = app_window.clone_handles();
        app_window.setup_menu_actions(&handles);

        app_window
    }

    pub fn present(&self) {
        self.window.present();
    }

    fn clone_handles(&self) -> WindowHandles {
        WindowHandles {
            window: self.window.clone(),
            toast_overlay: self.toast_overlay.clone(),
            view_stack: self.view_stack.clone(),
            editor_view: self.editor_view.clone(),
            diff_view: self.diff_view.clone(),
            spinner: self.spinner.clone(),
            status_label: self.status_label.clone(),
            is_busy: self.is_busy.clone(),
        }
    }

    fn setup_menu_actions(&self, handles: &WindowHandles) {
        let h_vd = handles.clone();
        let act_vd = gio::SimpleAction::new("sample_vardump", None);
        act_vd.connect_activate(move |_, _| {
            h_vd.editor_view.set_text(SAMPLE_VARDUMP);
            h_vd.editor_view.set_language(Some("php"));
            h_vd.view_stack.set_visible_child_name("editor");
            h_vd.show_toast("Ejemplo de var_dump cargado");
        });
        self.window.add_action(&act_vd);

        let h_js = handles.clone();
        let act_js = gio::SimpleAction::new("sample_json", None);
        act_js.connect_activate(move |_, _| {
            h_js.editor_view.set_text(SAMPLE_JSON);
            h_js.editor_view.set_language(Some("json"));
            h_js.view_stack.set_visible_child_name("editor");
            h_js.show_toast("Ejemplo de JSON cargado");
        });
        self.window.add_action(&act_js);

        let h_df = handles.clone();
        let act_df = gio::SimpleAction::new("sample_diff", None);
        act_df.connect_activate(move |_, _| {
            h_df.diff_view.left_editor().set_text(SAMPLE_DIFF_OLD);
            h_df.diff_view.right_editor().set_text(SAMPLE_DIFF_NEW);
            h_df.view_stack.set_visible_child_name("diff");
            h_df.diff_view.update_diff();
            h_df.show_toast("Ejemplo de Diff cargado");
        });
        self.window.add_action(&act_df);

        let h_sc = handles.clone();
        let act_sc = gio::SimpleAction::new("shortcuts", None);
        act_sc.connect_activate(move |_, _| {
            h_sc.show_shortcuts_dialog();
        });
        self.window.add_action(&act_sc);

        let win_clone = self.window.clone();
        let act_ab = gio::SimpleAction::new("about", None);
        act_ab.connect_activate(move |_, _| {
            let about = adw::AboutDialog::builder()
                .application_name("VarLens")
                .application_icon("io.github.varlens.App")
                .developer_name("VarLens Team")
                .version("0.1.0")
                .website("https://github.com/varlens/varlens")
                .issue_url("https://github.com/varlens/varlens/issues")
                .license_type(gtk4::License::MitX11)
                .comments("Herramienta de desarrollo nativa para Linux orientada a formatear y validar JSON, convertir salidas crudas de var_dump() de PHP a JSON y comparar diferencias visualmente.")
                .developers(vec!["VarLens Developers"])
                .build();
            about.present(Some(&win_clone));
        });
        self.window.add_action(&act_ab);
    }

    fn setup_actions(
        &self,
        btn_prettify: &gtk4::Button,
        btn_minify: &gtk4::Button,
        btn_validate: &gtk4::Button,
        btn_vardump: &gtk4::Button,
        btn_diff: &gtk4::Button,
        btn_copy: &gtk4::Button,
        btn_clear: &gtk4::Button,
    ) {
        let self_prettify = self.clone_handles();
        btn_prettify.connect_clicked(move |_| {
            self_prettify.handle_json_format(true);
        });

        let self_minify = self.clone_handles();
        btn_minify.connect_clicked(move |_| {
            self_minify.handle_json_format(false);
        });

        let self_validate = self.clone_handles();
        btn_validate.connect_clicked(move |_| {
            self_validate.handle_json_validate();
        });

        let self_vardump = self.clone_handles();
        btn_vardump.connect_clicked(move |_| {
            self_vardump.handle_vardump_convert();
        });

        let self_diff = self.clone_handles();
        btn_diff.connect_clicked(move |_| {
            self_diff.handle_diff_action();
        });

        let self_copy = self.clone_handles();
        btn_copy.connect_clicked(move |_| {
            self_copy.handle_copy();
        });

        let self_clear = self.clone_handles();
        btn_clear.connect_clicked(move |_| {
            self_clear.handle_clear();
        });
    }
}

/// Helper cloneable wrapper to dispatch actions to the UI safely
#[derive(Clone)]
struct WindowHandles {
    window: adw::ApplicationWindow,
    toast_overlay: adw::ToastOverlay,
    view_stack: adw::ViewStack,
    editor_view: EditorView,
    diff_view: DiffView,
    spinner: gtk4::Spinner,
    status_label: gtk4::Label,
    is_busy: Arc<AtomicBool>,
}

impl WindowHandles {
    fn show_toast(&self, text: &str) {
        let toast = adw::Toast::new(text);
        toast.set_timeout(3);
        self.toast_overlay.add_toast(toast);
    }

    fn set_busy(&self, busy: bool, message: &str) {
        self.is_busy.store(busy, Ordering::SeqCst);
        self.spinner.set_visible(busy);
        self.spinner.set_spinning(busy);
        self.status_label.set_label(message);
    }

    fn show_shortcuts_dialog(&self) {
        let dialog = adw::MessageDialog::builder()
            .heading("Atajos y Guía de Uso")
            .body("• Prettify: Formatear y validar JSON con sangrado legible\n• Minify: Comprimir JSON a una sola línea\n• Validar: Comprobar sintaxis sin alterar espaciado\n• var_dump ➔ JSON: Transformar estructuras PHP recursivas\n• Calcular Diff: Comparar lado a lado con Myers o Patience\n• Copiar: Copiar buffer actual o diff unificado\n• Limpiar: Vaciar panel activo\n• Hilos en 2do plano: Despacho automático para >1000 líneas")
            .transient_for(&self.window)
            .modal(true)
            .build();
        dialog.add_response("ok", "Entendido");
        dialog.set_default_response(Some("ok"));
        dialog.present();
    }

    fn active_mode(&self) -> String {
        self.view_stack
            .visible_child_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "editor".to_string())
    }

    fn handle_json_format(&self, pretty: bool) {
        let text = self.editor_view.text();
        let lines = count_lines(&text);

        if text.trim().is_empty() {
            self.show_toast("El editor está vacío");
            return;
        }

        self.view_stack.set_visible_child_name("editor");
        let action_name = if pretty { "Formateando JSON" } else { "Minificando JSON" };

        if lines > ASYNC_LINE_THRESHOLD {
            self.set_busy(true, &format!("{} (asíncrono, {} líneas)...", action_name, lines));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = match format_json(&text, pretty) {
                    Ok(formatted) => WorkerResult::EditorFormatted {
                        text: formatted,
                        message: if pretty {
                            "JSON formateado correctamente (Prettify)".to_string()
                        } else {
                            "JSON minificado correctamente (Minify)".to_string()
                        },
                    },
                    Err(e) => WorkerResult::Error(e),
                };
                glib::idle_add_once(move || {
                    handles.into_inner().handle_worker_result(res);
                });
            });
        } else {
            match format_json(&text, pretty) {
                Ok(formatted) => {
                    self.editor_view.set_text(&formatted);
                    self.show_toast(if pretty { "JSON formateado" } else { "JSON minificado" });
                    self.set_busy(false, "Listo");
                }
                Err(err) => {
                    self.show_toast(&err);
                    self.set_busy(false, "Error de validación JSON");
                }
            }
        }
    }

    fn handle_json_validate(&self) {
        let text = self.editor_view.text();
        if text.trim().is_empty() {
            self.show_toast("El editor está vacío");
            return;
        }

        self.view_stack.set_visible_child_name("editor");
        match validate_json(&text) {
            Ok(()) => {
                self.show_toast("JSON válido");
                self.set_busy(false, "JSON sintácticamente válido");
            }
            Err(err) => {
                self.show_toast(&err);
                self.set_busy(false, "JSON inválido");
            }
        }
    }

    fn handle_vardump_convert(&self) {
        let text = self.editor_view.text();
        let lines = count_lines(&text);

        if text.trim().is_empty() {
            self.show_toast("El editor está vacío");
            return;
        }

        self.view_stack.set_visible_child_name("editor");

        if lines > ASYNC_LINE_THRESHOLD {
            self.set_busy(true, &format!("Parseando var_dump (asíncrono, {} líneas)...", lines));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = match vardump_to_json_string(&text, true) {
                    Ok(json_str) => WorkerResult::EditorFormatted {
                        text: json_str,
                        message: "var_dump convertido exitosamente a JSON".to_string(),
                    },
                    Err(e) => WorkerResult::Error(e),
                };
                glib::idle_add_once(move || {
                    handles.into_inner().handle_worker_result(res);
                });
            });
        } else {
            match vardump_to_json_string(&text, true) {
                Ok(json_str) => {
                    self.editor_view.set_text(&json_str);
                    self.editor_view.set_language(Some("json"));
                    self.show_toast("var_dump convertido a JSON");
                    self.set_busy(false, "Listo");
                }
                Err(err) => {
                    self.show_toast("Error al parsear var_dump");
                    self.set_busy(false, &format!("Error: {}", err));
                }
            }
        }
    }

    fn handle_diff_action(&self) {
        self.view_stack.set_visible_child_name("diff");

        let left_text = self.diff_view.left_editor().text();
        let right_text = self.diff_view.right_editor().text();
        let patience = self.diff_view.is_patience_active();

        let total_lines = count_lines(&left_text) + count_lines(&right_text);

        if total_lines > ASYNC_LINE_THRESHOLD {
            self.set_busy(true, &format!("Calculando Diff (asíncrono, {} líneas)...", total_lines));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let summary = compute_diff(&left_text, &right_text, patience);
                glib::idle_add_once(move || {
                    handles.into_inner().handle_worker_result(WorkerResult::DiffCalculated(summary));
                });
            });
        } else {
            self.diff_view.update_diff();
            self.show_toast("Diff calculado");
            self.set_busy(false, "Listo");
        }
    }

    fn handle_copy(&self) {
        let clipboard = self.window.clipboard();
        let text_to_copy = if self.active_mode() == "diff" {
            let left = self.diff_view.left_editor().text();
            let right = self.diff_view.right_editor().text();
            let summary = compute_diff(&left, &right, self.diff_view.is_patience_active());
            summary.unified_diff
        } else {
            self.editor_view.text()
        };

        if text_to_copy.is_empty() {
            self.show_toast("Nada para copiar");
        } else {
            clipboard.set_text(&text_to_copy);
            self.show_toast("Copiado al portapapeles");
        }
    }

    fn handle_clear(&self) {
        if self.active_mode() == "diff" {
            self.diff_view.left_editor().clear();
            self.diff_view.right_editor().clear();
            self.diff_view.clear_highlights();
        } else {
            self.editor_view.clear();
        }
        self.show_toast("Editor limpiado");
        self.set_busy(false, "Listo");
    }

    fn handle_worker_result(&self, result: WorkerResult) {
        match result {
            WorkerResult::EditorFormatted { text, message } => {
                self.editor_view.set_text(&text);
                self.editor_view.set_language(Some("json"));
                self.show_toast(&message);
                self.set_busy(false, "Operación completada");
            }
            WorkerResult::DiffCalculated(summary) => {
                self.diff_view.apply_diff_summary(summary);
                self.show_toast("Diff calculado en segundo plano");
                self.set_busy(false, "Listo");
            }
            WorkerResult::Error(err) => {
                self.show_toast("Error en la operación");
                self.set_busy(false, &format!("Error: {}", err));
            }
        }
    }
}
