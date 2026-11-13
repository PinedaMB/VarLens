use super::*;

impl VarDiffWindow {
    pub(super) fn set_buffer_language(buf: &gtk4::TextBuffer, lang_id: &str) {
        let lang_mgr = sourceview5::LanguageManager::default();
        if let Some(sbuf) = buf.downcast_ref::<sourceview5::Buffer>() {
            let lang = lang_mgr.language(lang_id);
            sbuf.set_language(lang.as_ref());
        }
    }

    pub(super) fn apply_schemes(
        left_buf: &gtk4::TextBuffer,
        right_buf: &gtk4::TextBuffer,
        inspector_buf: &gtk4::TextBuffer,
        is_dark: bool,
    ) {
        if let Some(sbuf) = left_buf.downcast_ref::<sourceview5::Buffer>() {
            EditorView::update_scheme(sbuf, is_dark);
        }
        if let Some(sbuf) = right_buf.downcast_ref::<sourceview5::Buffer>() {
            EditorView::update_scheme(sbuf, is_dark);
        }
        if let Some(sbuf) = inspector_buf.downcast_ref::<sourceview5::Buffer>() {
            EditorView::update_scheme(sbuf, is_dark);
        }
    }

    pub(super) fn current_indent_style(&self) -> IndentStyle {
        match self.imp().dd_indent.selected() {
            1 => IndentStyle::Spaces4,
            2 => IndentStyle::Tabs,
            _ => IndentStyle::Spaces2,
        }
    }

    pub(super) fn handle_open_file(&self, force_left_diff: bool) {
        let target = if force_left_diff {
            FileTarget::DiffLeft
        } else {
            FileTarget::Active
        };
        self.handle_open_file_target(target);
    }

    pub(super) fn handle_open_file_target(&self, target: FileTarget) {
        let file_dialog = gtk4::FileDialog::builder()
            .title(tr!("Abrir archivo", "Open file"))
            .modal(true)
            .build();

        let self_clone = self.clone();
        file_dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                let filename = file
                    .basename()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "archivo".to_string());

                if let Ok((contents, _)) = file.load_contents(gio::Cancellable::NONE) {
                    if let Ok(text) = String::from_utf8(contents.to_vec()) {
                        let imp = self_clone.imp();
                        match target {
                            FileTarget::DiffLeft => {
                                imp.source_diff_left.buffer().set_text(&text);
                                imp.lbl_left_file.set_label(&filename);
                                self_clone.handle_compute_diff();
                            }
                            FileTarget::DiffRight => {
                                imp.source_diff_right.buffer().set_text(&text);
                                imp.lbl_right_file.set_label(&filename);
                                self_clone.handle_compute_diff();
                            }
                            FileTarget::Active => {
                                let active = imp.main_stack.visible_child_name().unwrap_or_default();
                                if active == "diff_page" {
                                    let left_buf = imp.source_diff_left.buffer();
                                    let right_buf = imp.source_diff_right.buffer();
                                    if left_buf.char_count() == 0 {
                                        left_buf.set_text(&text);
                                        imp.lbl_left_file.set_label(&filename);
                                    } else {
                                        right_buf.set_text(&text);
                                        imp.lbl_right_file.set_label(&filename);
                                        self_clone.handle_compute_diff();
                                    }
                                } else {
                                    if filename.ends_with(".xml") {
                                        Self::set_buffer_language(&imp.source_inspector.buffer(), "xml");
                                        imp.btn_mode_xml.set_active(true);
                                    }
                                    imp.source_inspector.buffer().set_text(&text);
                                    imp.lbl_inspector_file.set_label(&filename);
                                    imp.lbl_inspector_status.set_label(&format!(
                                        "{}: {} {}",
                                        tr!("Archivo cargado", "File loaded"),
                                        count_lines(&text),
                                        tr!("líneas", "lines")
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub(super) fn setup_drag_and_drop(&self) {
        let imp = self.imp();

        let attach_drop = |widget: &gtk4::Widget, win: Self, target: FileTarget| {
            let drop_target =
                gtk4::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
            drop_target.connect_drop(move |_, value, _, _| {
                if let Ok(file) = value.get::<gio::File>() {
                    win.load_gio_file(&file, target);
                    return true;
                }
                false
            });
            widget.add_controller(drop_target);
        };

        attach_drop(
            imp.source_inspector.upcast_ref::<gtk4::Widget>(),
            self.clone(),
            FileTarget::Active,
        );
        attach_drop(
            imp.source_diff_left.upcast_ref::<gtk4::Widget>(),
            self.clone(),
            FileTarget::DiffLeft,
        );
        attach_drop(
            imp.source_diff_right.upcast_ref::<gtk4::Widget>(),
            self.clone(),
            FileTarget::DiffRight,
        );
    }

    pub fn load_inspector_text(&self, text: &str, source_name: &str) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name("inspector_page");
        let buf = imp.source_inspector.buffer();
        let trimmed = text.trim_start();
        if source_name.ends_with(".xml") || trimmed.starts_with('<') {
            Self::set_buffer_language(&buf, "xml");
            imp.btn_mode_xml.set_active(true);
        } else if source_name.ends_with(".php")
            || crate::core::php_serialize::is_php_serialized(trimmed)
            || (serde_json::from_str::<serde_json::Value>(trimmed).is_err()
                && parse_vardump(trimmed).is_ok())
        {
            Self::set_buffer_language(&buf, "php");
            if is_printr_raw(trimmed) {
                imp.btn_mode_raw.set_active(true);
            } else {
                imp.btn_mode_vardump.set_active(true);
            }
        } else {
            Self::set_buffer_language(&buf, "json");
            imp.btn_mode_json.set_active(true);
        }
        buf.set_text(text);
        imp.lbl_inspector_file.set_label(source_name);
        imp.lbl_inspector_status.set_label(&format!(
            "{}: {} {}",
            tr!("Cargado", "Loaded"),
            count_lines(text),
            tr!("líneas", "lines")
        ));
    }

    pub fn open_inspector_file(&self, file: &gio::File) {
        self.imp().main_stack.set_visible_child_name("inspector_page");
        self.load_gio_file(file, FileTarget::Active);
    }

    pub fn open_diff_files(&self, left: &gio::File, right: &gio::File) {
        self.imp().main_stack.set_visible_child_name("diff_page");
        self.load_gio_file(left, FileTarget::DiffLeft);
        self.load_gio_file(right, FileTarget::DiffRight);
    }

    pub(super) fn load_gio_file(&self, file: &gio::File, target: FileTarget) {
        let filename = file
            .basename()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| tr!("archivo", "file").to_string());

        if let Ok((contents, _)) = file.load_contents(gio::Cancellable::NONE) {
            if let Ok(text) = String::from_utf8(contents.to_vec()) {
                let imp = self.imp();
                match target {
                    FileTarget::DiffLeft => {
                        imp.source_diff_left.buffer().set_text(&text);
                        imp.lbl_left_file.set_label(&filename);
                        self.handle_compute_diff();
                    }
                    FileTarget::DiffRight => {
                        imp.source_diff_right.buffer().set_text(&text);
                        imp.lbl_right_file.set_label(&filename);
                        self.handle_compute_diff();
                    }
                    FileTarget::Active => {
                        let active = imp.main_stack.visible_child_name().unwrap_or_default();
                        if active == "diff_page" {
                            let left_buf = imp.source_diff_left.buffer();
                            let right_buf = imp.source_diff_right.buffer();
                            if left_buf.char_count() == 0 {
                                left_buf.set_text(&text);
                                imp.lbl_left_file.set_label(&filename);
                            } else {
                                right_buf.set_text(&text);
                                imp.lbl_right_file.set_label(&filename);
                                self.handle_compute_diff();
                            }
                        } else {
                            if filename.ends_with(".xml") {
                                Self::set_buffer_language(&imp.source_inspector.buffer(), "xml");
                                imp.btn_mode_xml.set_active(true);
                            }
                            imp.source_inspector.buffer().set_text(&text);
                            imp.lbl_inspector_file.set_label(&filename);
                            imp.lbl_inspector_status.set_label(&format!(
                                "{}: {} {}",
                                tr!("Archivo cargado", "File loaded"),
                                count_lines(&text),
                                tr!("líneas", "lines")
                            ));
                        }
                    }
                }
            }
        }
    }

    pub(super) fn handle_save_file(&self) {
        let imp = self.imp();
        let active = imp.main_stack.visible_child_name().unwrap_or_default();
        let ext_for_text = |s: &str| -> &'static str {
            match Self::detect_payload_format(s) {
                "XML" => "xml",
                "PHP var_dump" | "Array raw" => "php",
                "JSON" => "json",
                _ => "txt",
            }
        };

        let (text, default_name) = if active == "diff_page" {
            let right_buf = imp.source_diff_right.buffer();
            let r_text = right_buf
                .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
                .to_string();
            if !r_text.trim().is_empty() {
                let ext = ext_for_text(&r_text);
                (r_text, format!("modified.{}", ext))
            } else {
                let left_buf = imp.source_diff_left.buffer();
                let l_text = left_buf
                    .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
                    .to_string();
                let ext = ext_for_text(&l_text);
                (l_text, format!("original.{}", ext))
            }
        } else {
            let buf = imp.source_inspector.buffer();
            let name = if imp.btn_mode_xml.is_active() {
                "payload.xml".to_string()
            } else if imp.btn_mode_vardump.is_active() || imp.btn_mode_raw.is_active() {
                "payload.php".to_string()
            } else {
                "payload.json".to_string()
            };
            (
                buf.text(&buf.start_iter(), &buf.end_iter(), true)
                    .to_string(),
                name,
            )
        };

        if text.trim().is_empty() {
            if active == "diff_page" {
                imp.lbl_diff_sync
                    .set_label(tr!("El editor está vacío", "Editor is empty"));
            } else {
                imp.lbl_inspector_status
                    .set_label(tr!("El editor está vacío", "Editor is empty"));
            }
            return;
        }

        let file_dialog = gtk4::FileDialog::builder()
            .title(tr!("Guardar archivo", "Save file"))
            .initial_name(&default_name)
            .modal(true)
            .build();

        let self_clone = self.clone();
        file_dialog.save(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    if std::fs::write(&path, &text).is_ok() {
                        let filename = file
                            .basename()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let imp = self_clone.imp();
                        imp.lbl_inspector_file.set_label(&filename);
                        imp.lbl_inspector_status.set_label(&format!(
                            "{}: {}",
                            tr!("Guardado", "Saved"),
                            filename
                        ));
                    }
                }
            }
        });
    }
}
