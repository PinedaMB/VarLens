use gtk4::prelude::*;
use sourceview5::prelude::*;

#[derive(Clone)]
#[allow(dead_code)]
pub struct EditorView {
    container: gtk4::ScrolledWindow,
    view: sourceview5::View,
    buffer: sourceview5::Buffer,
}

#[allow(dead_code)]
impl EditorView {
    /// Creates a new configured code editor view.
    pub fn new(language_id: Option<&str>) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        let view = sourceview5::View::with_buffer(&buffer);

        // Configure code editor features
        view.set_show_line_numbers(true);
        view.set_highlight_current_line(true);
        view.set_tab_width(2);
        view.set_indent_width(2);
        view.set_insert_spaces_instead_of_tabs(true);
        view.set_auto_indent(true);
        view.set_monospace(true);
        view.set_enable_snippets(false);

        // Set Language if provided
        if let Some(lang) = language_id {
            let lang_mgr = sourceview5::LanguageManager::default();
            if let Some(language) = lang_mgr.language(lang) {
                buffer.set_language(Some(&language));
            }
        }

        // Apply matching Adwaita color scheme
        Self::update_scheme(&buffer, false);

        let container = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .build();

        Self {
            container,
            view,
            buffer,
        }
    }

    /// Updates the style scheme according to dark / light preference.
    pub fn update_scheme(buffer: &sourceview5::Buffer, is_dark: bool) {
        let scheme_mgr = sourceview5::StyleSchemeManager::default();
        let scheme_id = if is_dark { "varlens-dark" } else { "varlens-light" };

        if let Some(scheme) = scheme_mgr.scheme(scheme_id) {
            buffer.set_style_scheme(Some(&scheme));
        } else if let Some(fallback) = scheme_mgr.scheme(if is_dark { "Adwaita-dark" } else { "Adwaita" }) {
            buffer.set_style_scheme(Some(&fallback));
        } else if let Some(fallback_classic) = scheme_mgr.scheme(if is_dark { "classic-dark" } else { "classic" }) {
            buffer.set_style_scheme(Some(&fallback_classic));
        }
    }

    /// Sets the programming language for syntax highlighting.
    pub fn set_language(&self, language_id: Option<&str>) {
        if let Some(lang) = language_id {
            let lang_mgr = sourceview5::LanguageManager::default();
            let language = lang_mgr.language(lang);
            self.buffer.set_language(language.as_ref());
        } else {
            self.buffer.set_language(None);
        }
    }

    /// Returns the main container widget (ScrolledWindow).
    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.container
    }

    /// Returns the underlying GtkSourceView.
    #[allow(dead_code)]
    pub fn view(&self) -> &sourceview5::View {
        &self.view
    }

    /// Returns the underlying GtkSourceBuffer.
    pub fn buffer(&self) -> &sourceview5::Buffer {
        &self.buffer
    }

    /// Retrieves all text in the buffer.
    pub fn text(&self) -> String {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.text(&start, &end, true).to_string()
    }

    /// Replaces the entire buffer text with new content.
    pub fn set_text(&self, text: &str) {
        self.buffer.set_text(text);
    }

    /// Clears the editor.
    pub fn clear(&self) {
        self.buffer.set_text("");
    }

    /// Returns the number of lines currently in the buffer.
    #[allow(dead_code)]
    pub fn line_count(&self) -> usize {
        self.buffer.line_count() as usize
    }

    /// Sets whether the editor buffer is editable.
    #[allow(dead_code)]
    pub fn set_editable(&self, editable: bool) {
        self.view.set_editable(editable);
    }
}
