use super::*;

impl VarDiffWindow {
    pub(super) fn update_inspector_metrics(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();

        if text.trim().is_empty() {
            self.clear_inspector_error_highlight();
            imp.lbl_syntax_valid.set_label(tr!("Listo", "Ready"));
            imp.lbl_syntax_valid.remove_css_class("badge-error");
            imp.lbl_syntax_valid.remove_css_class("badge-success");
            imp.lbl_node_count.set_label(&format!("0 {}", tr!("nodos", "nodes")));
            imp.lbl_payload_size.set_label("0 B");
            imp.lbl_tree_stats.set_label(&format!(
                "{}: 0 {} | 0 {}",
                tr!("Árbol", "Tree"),
                tr!("niveles", "levels"),
                tr!("elementos", "items")
            ));
            imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
            imp.lbl_inspector_status.set_tooltip_text(None);
            imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear (Ctrl+Shift+F)", "Format (Ctrl+Shift+F)")));
            imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar", "Minify")));
            imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir a JSON", "Convert to JSON")));
            imp.btn_sort_keys.set_sensitive(true);
            return;
        }

        if imp.btn_mode_xml.is_active() {
            imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear XML (Ctrl+Shift+F)", "Format XML (Ctrl+Shift+F)")));
            imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar XML", "Minify XML")));
            imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir XML → JSON", "Convert XML → JSON")));
            imp.btn_sort_keys.set_sensitive(false);

            match analyze_xml(&text) {
                Ok(metrics) => {
                    self.clear_inspector_error_highlight();
                    imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                    imp.lbl_inspector_status.set_tooltip_text(None);

                    imp.lbl_syntax_valid.set_label(tr!("✓ XML Válido", "✓ Valid XML"));
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_node_count
                        .set_label(&format!("{} {}", metrics.total_elements, tr!("elementos", "elements")));
                    imp.lbl_payload_size.set_label(&metrics.formatted_size());
                    imp.lbl_tree_stats.set_label(&format!(
                        "{}: {} {} | {} {}",
                        tr!("Árbol XML", "XML Tree"),
                        metrics.max_depth,
                        tr!("niveles", "levels"),
                        metrics.total_attributes,
                        tr!("atributos", "attributes")
                    ));
                }
                Err(err_msg) => {
                    imp.lbl_syntax_valid.set_label(tr!("✗ XML Inválido", "✗ Invalid XML"));
                    imp.lbl_syntax_valid.remove_css_class("badge-success");
                    imp.lbl_syntax_valid.add_css_class("badge-error");

                    imp.lbl_inspector_status.set_label(&err_msg);
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err_msg));
                    if let Some((l, c)) = Self::extract_error_line_col(&err_msg) {
                        self.highlight_inspector_error(l, c);
                    }
                }
            }
            if imp.btn_insp_view_tree.is_active() {
                self.refresh_tree_view();
            }
            return;
        }

        if imp.btn_mode_raw.is_active() {
            imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear (Ctrl+Shift+F)", "Format (Ctrl+Shift+F)")));
            imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar", "Minify")));
            imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir Array raw → JSON", "Convert Array raw → JSON")));
            imp.btn_sort_keys.set_sensitive(false);

            match parse_vardump(&text) {
                Ok(val) => {
                    self.clear_inspector_error_highlight();
                    imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                    imp.lbl_inspector_status.set_tooltip_text(None);

                    let metrics = analyze_value(&val, text.len());
                    imp.lbl_syntax_valid.set_label(tr!("✓ Array raw Válido", "✓ Valid Array raw"));
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_node_count
                        .set_label(&format!("{} {}", metrics.total_nodes, tr!("nodos", "nodes")));
                    imp.lbl_payload_size.set_label(&metrics.formatted_size());
                    imp.lbl_tree_stats.set_label(&format!(
                        "{}: {} {} | {} {}",
                        tr!("Árbol PHP", "PHP Tree"),
                        metrics.max_depth,
                        tr!("niveles", "levels"),
                        metrics.total_keys,
                        tr!("claves", "keys")
                    ));
                }
                Err(err_msg) => {
                    imp.lbl_syntax_valid.set_label(tr!("✗ Array raw Inválido", "✗ Invalid Array raw"));
                    imp.lbl_syntax_valid.remove_css_class("badge-success");
                    imp.lbl_syntax_valid.add_css_class("badge-error");
                    if let Some((l, c)) = Self::extract_error_line_col(&err_msg) {
                        imp.lbl_inspector_status.set_label(&format!(
                            "{} {}, Col {}",
                            tr!("Línea", "Line"),
                            l,
                            c
                        ));
                        self.highlight_inspector_error(l, c);
                    } else {
                        self.clear_inspector_error_highlight();
                    }
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err_msg));
                }
            }
            if imp.btn_insp_view_tree.is_active() {
                self.refresh_tree_view();
            }
            return;
        }

        if imp.btn_mode_vardump.is_active() {
            imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear (Ctrl+Shift+F)", "Format (Ctrl+Shift+F)")));
            imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar", "Minify")));
            imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir var_dump → JSON", "Convert var_dump → JSON")));
            imp.btn_sort_keys.set_sensitive(false);

            match parse_vardump(&text) {
                Ok(val) => {
                    self.clear_inspector_error_highlight();
                    imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                    imp.lbl_inspector_status.set_tooltip_text(None);

                    let metrics = analyze_value(&val, text.len());
                    imp.lbl_syntax_valid.set_label(tr!("✓ var_dump Válido", "✓ Valid var_dump"));
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_node_count
                        .set_label(&format!("{} {}", metrics.total_nodes, tr!("nodos", "nodes")));
                    imp.lbl_payload_size.set_label(&metrics.formatted_size());
                    imp.lbl_tree_stats.set_label(&format!(
                        "{}: {} {} | {} {}",
                        tr!("Árbol PHP", "PHP Tree"),
                        metrics.max_depth,
                        tr!("niveles", "levels"),
                        metrics.total_keys,
                        tr!("claves", "keys")
                    ));
                }
                Err(err_msg) => {
                    imp.lbl_syntax_valid.set_label(tr!("✗ var_dump Inválido", "✗ Invalid var_dump"));
                    imp.lbl_syntax_valid.remove_css_class("badge-success");
                    imp.lbl_syntax_valid.add_css_class("badge-error");
                    if let Some((l, c)) = Self::extract_error_line_col(&err_msg) {
                        imp.lbl_inspector_status.set_label(&format!(
                            "{} {}, Col {}",
                            tr!("Línea", "Line"),
                            l,
                            c
                        ));
                        self.highlight_inspector_error(l, c);
                    } else {
                        self.clear_inspector_error_highlight();
                    }
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err_msg));
                }
            }
            if imp.btn_insp_view_tree.is_active() {
                self.refresh_tree_view();
            }
            return;
        }

        imp.btn_sort_keys.set_sensitive(true);
        match analyze_json(&text) {
            Ok(metrics) => {
                self.clear_inspector_error_highlight();
                imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                imp.lbl_inspector_status.set_tooltip_text(None);

                imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear JSON (Ctrl+Shift+F)", "Format JSON (Ctrl+Shift+F)")));
                imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar JSON", "Minify JSON")));
                imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir a JSON", "Convert to JSON")));

                imp.lbl_syntax_valid.set_label(tr!("✓ Sintaxis Válida", "✓ Valid Syntax"));
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_node_count
                    .set_label(&format!("{} {}", metrics.total_nodes, tr!("nodos", "nodes")));
                imp.lbl_payload_size.set_label(&metrics.formatted_size());
                imp.lbl_tree_stats.set_label(&format!(
                    "{}: {} {} | {} {}",
                    tr!("Árbol", "Tree"),
                    metrics.max_depth,
                    tr!("niveles", "levels"),
                    metrics.total_keys,
                    tr!("claves", "keys")
                ));
            }
            Err(err_loc) => {
                // Auto-detection fallback when in JSON mode but content is XML or var_dump
                if text.trim_start().starts_with('<') && analyze_xml(&text).is_ok() {
                    Self::set_buffer_language(&buf, "xml");
                    imp.btn_mode_xml.set_active(true);
                    return;
                }
                if parse_vardump(&text).is_ok() {
                    Self::set_buffer_language(&buf, "php");
                    if is_printr_raw(&text) {
                        imp.btn_mode_raw.set_active(true);
                    } else {
                        imp.btn_mode_vardump.set_active(true);
                    }
                    return;
                }

                imp.lbl_syntax_valid.set_label(tr!("✗ Sintaxis Inválida", "✗ Invalid Syntax"));
                imp.lbl_syntax_valid.remove_css_class("badge-success");
                imp.lbl_syntax_valid.add_css_class("badge-error");

                if let Err(json_err) = serde_json::from_str::<serde_json::Value>(&text) {
                    let l = json_err.line();
                    let c = json_err.column();
                    imp.lbl_inspector_status.set_label(&format!(
                        "{} {}, Col {}",
                        tr!("Línea", "Line"),
                        l,
                        c
                    ));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&json_err.to_string()));
                    self.highlight_inspector_error(l, c);
                } else {
                    imp.lbl_inspector_status.set_label(&err_loc);
                    if let Some((l, c)) = Self::extract_error_line_col(&err_loc) {
                        self.highlight_inspector_error(l, c);
                    }
                }
            }
        }

        if imp.btn_insp_view_tree.is_active() {
            self.refresh_tree_view();
        }
    }

    pub(super) fn clear_inspector_error_highlight(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        if let Some(tag) = imp.tag_syntax_error.get() {
            buf.remove_tag(tag, &buf.start_iter(), &buf.end_iter());
        }
        if let Some(fold) = imp.fold_inspector.get() {
            fold.set_error_line(None);
        }
    }

    pub(super) fn highlight_inspector_error(&self, line_1based: usize, col_1based: usize) {
        self.clear_inspector_error_highlight();
        let imp = self.imp();
        let line_0 = line_1based.saturating_sub(1);
        if let Some(fold) = imp.fold_inspector.get() {
            fold.set_error_line(Some(line_0));
        }

        let buf = imp.source_inspector.buffer();
        let Some(tag) = imp.tag_syntax_error.get() else {
            return;
        };

        let line_0 = line_0 as i32;
        let Some(line_start) = buf.iter_at_line(line_0) else {
            return;
        };

        let mut line_end = line_start;
        if !line_end.ends_line() {
            line_end.forward_to_line_end();
        }
        let line_len = line_end.line_offset();
        let col_0 = (col_1based.saturating_sub(1) as i32).min(line_len);

        if let Some(mut start) = buf.iter_at_line_offset(line_0, col_0) {
            let mut end = start;
            if (!end.ends_line() && end.forward_char())
                || (start.line_offset() > 0 && start.backward_char())
            {
                buf.apply_tag(tag, &start, &end);
            }
        }
    }

    pub(super) fn extract_error_line_col(msg: &str) -> Option<(usize, usize)> {
        // Pest format: "--> 2:1"
        if let Some(idx) = msg.find("--> ") {
            let rest = msg[idx + 4..].trim_start();
            let token = rest.split_whitespace().next().unwrap_or("");
            if let Some((l_str, c_str)) = token.split_once(':') {
                if let (Ok(l), Ok(c)) = (l_str.parse::<usize>(), c_str.parse::<usize>()) {
                    return Some((l, c));
                }
            }
        }
        // Spanish/English format: "línea 2, columna 1" / "Línea 2, Col 1"
        let lower = msg.to_lowercase();
        let nums: Vec<usize> = lower
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<usize>().ok())
            .collect();
        if (lower.contains("línea") || lower.contains("linea") || lower.contains("line"))
            && nums.len() >= 2
        {
            return Some((nums[0], nums[1]));
        }
        None
    }

    pub(super) fn switch_to_tree_view(&self) {
        let imp = self.imp();
        imp.stack_inspector_view.set_visible_child_name("page_tree");
        self.refresh_tree_view();
    }

    pub(super) fn refresh_tree_view(&self) {
        let imp = self.imp();
        if let Some(tree_view) = imp.json_tree.get() {
            let buf = imp.source_inspector.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();

            if text.trim().is_empty() {
                return;
            }

            let parsed_json = if imp.btn_mode_json.is_active() {
                serde_json::from_str::<serde_json::Value>(&text).ok()
            } else if imp.btn_mode_vardump.is_active() || imp.btn_mode_raw.is_active() {
                crate::core::vardump::parse_vardump(&text).ok()
            } else if imp.btn_mode_xml.is_active() {
                crate::core::xml::xml_to_json_string(&text, false)
                    .ok()
                    .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
            } else {
                serde_json::from_str::<serde_json::Value>(&text).ok()
            };

            if let Some(val) = parsed_json {
                tree_view.load_json(&val);
            }
        }
    }

    pub(super) fn setup_code_search(&self) {
        let imp = self.imp();
        let Some(sbuf) = imp
            .source_inspector
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .ok()
        else {
            return;
        };

        let settings = sourceview5::SearchSettings::new();
        settings.set_case_sensitive(false);
        settings.set_wrap_around(true);

        let context = sourceview5::SearchContext::new(&sbuf, Some(&settings));
        context.set_highlight(true);
        imp.search_context.set(context.clone()).ok();

        let view = imp.source_inspector.get();
        let view_changed = view.clone();
        let ctx_changed = context.clone();
        let settings_changed = settings.clone();
        imp.entry_search_inspector
            .connect_search_changed(move |entry| {
                let query = entry.text();
                if query.is_empty() {
                    settings_changed.set_search_text(None);
                    return;
                }
                settings_changed.set_search_text(Some(query.as_str()));
                let buf = view_changed.buffer();
                let start = buf.start_iter();
                if let Some((match_start, match_end, _)) = ctx_changed.forward(&start) {
                    buf.select_range(&match_start, &match_end);
                    let mut scroll_iter = match_start;
                    view_changed.scroll_to_iter(&mut scroll_iter, 0.2, false, 0.0, 0.5);
                }
            });

        let view_act = view.clone();
        let ctx_act = context.clone();
        imp.entry_search_inspector.connect_activate(move |_| {
            let buf = view_act.buffer();
            let cursor = if let Some((_, sel_end)) = buf.selection_bounds() {
                sel_end
            } else {
                buf.iter_at_offset(buf.cursor_position())
            };
            if let Some((match_start, match_end, _)) = ctx_act.forward(&cursor) {
                buf.select_range(&match_start, &match_end);
                let mut scroll_iter = match_start;
                view_act.scroll_to_iter(&mut scroll_iter, 0.2, false, 0.0, 0.5);
            }
        });

        let view_next = view.clone();
        let ctx_next = context.clone();
        imp.entry_search_inspector.connect_next_match(move |_| {
            let buf = view_next.buffer();
            let cursor = if let Some((_, sel_end)) = buf.selection_bounds() {
                sel_end
            } else {
                buf.iter_at_offset(buf.cursor_position())
            };
            if let Some((match_start, match_end, _)) = ctx_next.forward(&cursor) {
                buf.select_range(&match_start, &match_end);
                let mut scroll_iter = match_start;
                view_next.scroll_to_iter(&mut scroll_iter, 0.2, false, 0.0, 0.5);
            }
        });

        let view_prev = view;
        let ctx_prev = context;
        imp.entry_search_inspector.connect_previous_match(move |_| {
            let buf = view_prev.buffer();
            let cursor = if let Some((sel_start, _)) = buf.selection_bounds() {
                sel_start
            } else {
                buf.iter_at_offset(buf.cursor_position())
            };
            if let Some((match_start, match_end, _)) = ctx_prev.backward(&cursor) {
                buf.select_range(&match_start, &match_end);
                let mut scroll_iter = match_start;
                view_prev.scroll_to_iter(&mut scroll_iter, 0.2, false, 0.0, 0.5);
            }
        });
    }

    pub(super) fn handle_sort_json_keys(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        if text.trim().is_empty() {
            imp.lbl_inspector_status
                .set_label(tr!("El editor está vacío", "Editor is empty"));
            return;
        }

        if imp.btn_mode_xml.is_active()
            || imp.btn_mode_vardump.is_active()
            || imp.btn_mode_raw.is_active()
        {
            imp.lbl_inspector_status.set_label(tr!(
                "Ordenar claves solo está disponible en modo JSON",
                "Sort keys is only available in JSON mode"
            ));
            return;
        }

        let indent = self.current_indent_style();
        match sort_json_keys(&text, indent) {
            Ok(sorted) => {
                buf.set_text(&sorted);
                Self::set_buffer_language(&buf, "json");
                imp.btn_mode_json.set_active(true);
                imp.lbl_inspector_status.set_label(tr!(
                    "Claves JSON ordenadas alfabéticamente (A→Z)",
                    "JSON keys sorted alphabetically (A→Z)"
                ));
                imp.lbl_inspector_status.set_tooltip_text(None);
            }
            Err(err) => {
                imp.lbl_inspector_status
                    .set_label(&format!("Error: {}", err));
                imp.lbl_inspector_status.set_tooltip_text(Some(&err));
            }
        }
    }

    pub(super) fn handle_json_format(&self, pretty: bool) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let lines = count_lines(&text);
        let indent = self.current_indent_style();

        if text.trim().is_empty() {
            imp.lbl_inspector_status
                .set_label(tr!("El editor está vacío", "Editor is empty"));
            return;
        }

        let is_xml = imp.btn_mode_xml.is_active() || text.trim_start().starts_with('<');
        let is_php = !is_xml
            && (imp.btn_mode_vardump.is_active()
                || imp.btn_mode_raw.is_active()
                || (serde_json::from_str::<serde_json::Value>(text.trim()).is_err()
                    && parse_vardump(&text).is_ok()));
        let php_mode = if is_printr_raw(&text) || imp.btn_mode_raw.is_active() {
            InspectorFormatMode::PhpRaw
        } else {
            InspectorFormatMode::PhpVarDump
        };

        let action_name = if is_xml {
            if pretty {
                tr!("Formateando XML", "Formatting XML")
            } else {
                tr!("Minificando XML", "Minifying XML")
            }
        } else if is_php {
            if pretty {
                tr!("Formateando PHP", "Formatting PHP")
            } else {
                tr!("Minificando PHP", "Minifying PHP")
            }
        } else if pretty {
            tr!("Formateando JSON", "Formatting JSON")
        } else {
            tr!("Minificando JSON", "Minifying JSON")
        };

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status.set_label(&format!(
                "{} ({} {})...",
                action_name,
                lines,
                tr!("líneas", "lines")
            ));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = if is_xml {
                    match format_xml_with_indent(&text, pretty, indent) {
                        Ok(formatted) => WorkerResult::InspectorFormatted {
                            text: formatted,
                            message: if pretty {
                                tr!("XML formateado correctamente", "XML formatted successfully")
                                    .to_string()
                            } else {
                                tr!("XML minificado correctamente", "XML minified successfully")
                                    .to_string()
                            },
                            mode: InspectorFormatMode::Xml,
                        },
                        Err(e) => WorkerResult::Error(e),
                    }
                } else if is_php {
                    match format_vardump_with_indent(&text, pretty, indent) {
                        Ok(formatted) => WorkerResult::InspectorFormatted {
                            text: formatted,
                            message: if pretty {
                                tr!("PHP formateado correctamente", "PHP formatted successfully")
                                    .to_string()
                            } else {
                                tr!("PHP minificado correctamente", "PHP minified successfully")
                                    .to_string()
                            },
                            mode: php_mode,
                        },
                        Err(e) => WorkerResult::Error(e),
                    }
                } else {
                    match format_json_with_indent(&text, pretty, indent) {
                        Ok(formatted) => WorkerResult::InspectorFormatted {
                            text: formatted,
                            message: if pretty {
                                tr!("JSON formateado correctamente", "JSON formatted successfully")
                                    .to_string()
                            } else {
                                tr!("JSON minificado correctamente", "JSON minified successfully")
                                    .to_string()
                            },
                            mode: InspectorFormatMode::Json,
                        },
                        Err(e) => WorkerResult::Error(e),
                    }
                };
                glib::idle_add_once(move || {
                    handles.into_inner().handle_worker_result(res);
                });
            });
        } else if is_xml {
            match format_xml_with_indent(&text, pretty, indent) {
                Ok(formatted) => {
                    buf.set_text(&formatted);
                    Self::set_buffer_language(&buf, "xml");
                    imp.btn_mode_xml.set_active(true);
                    imp.lbl_inspector_status.set_label(if pretty {
                        tr!("XML formateado", "XML formatted")
                    } else {
                        tr!("XML minificado", "XML minified")
                    });
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status.set_label(&format!("Error: {}", err));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
        } else if is_php {
            match format_vardump_with_indent(&text, pretty, indent) {
                Ok(formatted) => {
                    buf.set_text(&formatted);
                    Self::set_buffer_language(&buf, "php");
                    match php_mode {
                        InspectorFormatMode::PhpRaw => imp.btn_mode_raw.set_active(true),
                        _ => imp.btn_mode_vardump.set_active(true),
                    }
                    imp.lbl_inspector_status.set_label(if pretty {
                        tr!("PHP formateado", "PHP formatted")
                    } else {
                        tr!("PHP minificado", "PHP minified")
                    });
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(&format!("Error: {}", err));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
        } else {
            match format_json_with_indent(&text, pretty, indent) {
                Ok(formatted) => {
                    buf.set_text(&formatted);
                    Self::set_buffer_language(&buf, "json");
                    imp.btn_mode_json.set_active(true);
                    imp.lbl_inspector_status.set_label(if pretty {
                        tr!("JSON formateado", "JSON formatted")
                    } else {
                        tr!("JSON minificado", "JSON minified")
                    });
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(&format!("Error: {}", err));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
        }
    }

    pub(super) fn handle_vardump_convert(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let lines = count_lines(&text);

        if text.trim().is_empty() {
            imp.lbl_inspector_status.set_label(tr!("El editor está vacío", "Editor is empty"));
            imp.lbl_inspector_status.set_tooltip_text(None);
            return;
        }

        // Si es XML, convertir XML a JSON
        if imp.btn_mode_xml.is_active() || text.trim_start().starts_with('<') {
            match xml_to_json_string(&text, true) {
                Ok(json_str) => {
                    buf.set_text(&json_str);
                    Self::set_buffer_language(&buf, "json");
                    imp.btn_mode_json.set_active(true);
                    imp.lbl_inspector_status
                        .set_label(tr!("XML transformado exitosamente a JSON", "XML successfully converted to JSON"));
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(tr!("Error: No es un XML válido para convertir a JSON", "Error: Not a valid XML to convert to JSON"));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
            return;
        }

        // Si ya es un JSON válido, informar amablemente al usuario
        if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
            imp.lbl_inspector_status
                .set_label(tr!("El contenido ya es JSON válido (no requiere conversión)", "Content is already valid JSON (no conversion needed)"));
            imp.lbl_inspector_status.set_tooltip_text(None);
            return;
        }

        let is_raw = imp.btn_mode_raw.is_active();
        let mode_label = if is_raw { "Array raw" } else { "var_dump" };

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status
                .set_label(&format!("{} {} ({} {})...", tr!("Parseando", "Parsing"), mode_label, lines, tr!("líneas", "lines")));
            imp.lbl_inspector_status.set_tooltip_text(None);
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = match vardump_to_json_string(&text, true) {
                    Ok(json_str) => WorkerResult::InspectorFormatted {
                        text: json_str,
                        message: format!("{} {}", mode_label, tr!("transformado exitosamente a JSON", "successfully converted to JSON")),
                        mode: InspectorFormatMode::Json,
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
                    buf.set_text(&json_str);
                    Self::set_buffer_language(&buf, "json");
                    imp.btn_mode_json.set_active(true);
                    imp.lbl_inspector_status
                        .set_label(&format!("{} {}", mode_label, tr!("convertido a JSON", "converted to JSON")));
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(&format!("Error: {} PHP {} {}", tr!("No es una estructura", "Not a valid"), mode_label, tr!("válida", "structure")));
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
        }
    }
}
