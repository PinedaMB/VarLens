use super::*;

impl VarDiffWindow {
    pub(super) fn sync_scrolling(left: &gtk4::ScrolledWindow, right: &gtk4::ScrolledWindow) {
        let left_adj = left.vadjustment();
        let right_adj = right.vadjustment();

        let updating = Arc::new(AtomicBool::new(false));

        let up1 = updating.clone();
        let right_adj_clone = right_adj.clone();
        left_adj.connect_value_changed(move |adj| {
            if up1.load(Ordering::SeqCst) {
                return;
            }
            up1.store(true, Ordering::SeqCst);
            right_adj_clone.set_value(adj.value());
            up1.store(false, Ordering::SeqCst);
        });

        let up2 = updating;
        let left_adj_clone = left_adj.clone();
        right_adj.connect_value_changed(move |adj| {
            if up2.load(Ordering::SeqCst) {
                return;
            }
            up2.store(true, Ordering::SeqCst);
            left_adj_clone.set_value(adj.value());
            up2.store(false, Ordering::SeqCst);
        });
    }

    pub(super) fn clear_diff_highlights(&self) {
        let imp = self.imp();
        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();

        imp.source_diff_left.clear_diff_spans();
        imp.source_diff_right.clear_diff_spans();

        if let Some(tag_del) = imp.tag_deleted.get() {
            left_buf.remove_tag(tag_del, &left_buf.start_iter(), &left_buf.end_iter());
        }
        if let Some(tag_sdel) = imp.tag_span_deleted.get() {
            left_buf.remove_tag(tag_sdel, &left_buf.start_iter(), &left_buf.end_iter());
        }
        if let Some(tag_ins) = imp.tag_inserted.get() {
            right_buf.remove_tag(tag_ins, &right_buf.start_iter(), &right_buf.end_iter());
        }
        if let Some(tag_sins) = imp.tag_span_inserted.get() {
            right_buf.remove_tag(tag_sins, &right_buf.start_iter(), &right_buf.end_iter());
        }
    }

    pub(super) fn jump_diff_chunk(&self, next: bool) {
        let imp = self.imp();
        let hunks = imp.hunk_positions.borrow();
        if hunks.is_empty() {
            return;
        }

        let mut idx = imp.current_hunk_idx.get();
        if next {
            idx = (idx + 1) % hunks.len();
        } else {
            idx = if idx == 0 { hunks.len() - 1 } else { idx - 1 };
        }
        imp.current_hunk_idx.set(idx);

        let (left_line, right_line) = hunks[idx];

        let left_buf = imp.source_diff_left.buffer();
        if let Some(mut iter) = left_buf.iter_at_line(left_line as i32) {
            left_buf.place_cursor(&iter);
            imp.source_diff_left
                .scroll_to_iter(&mut iter, 0.2, false, 0.0, 0.5);
        }

        let right_buf = imp.source_diff_right.buffer();
        if let Some(mut iter) = right_buf.iter_at_line(right_line as i32) {
            right_buf.place_cursor(&iter);
            imp.source_diff_right
                .scroll_to_iter(&mut iter, 0.2, false, 0.0, 0.5);
        }
    }

    pub(super) fn schedule_diff_update(&self) {
        let imp = self.imp();
        if let Some(source_id) = imp.diff_debounce_source.borrow_mut().take() {
            source_id.remove();
        }

        let self_clone = self.clone();
        let new_source = glib::timeout_add_local_once(Duration::from_millis(150), move || {
            self_clone.imp().diff_debounce_source.borrow_mut().take();
            self_clone.handle_compute_diff();
        });

        *imp.diff_debounce_source.borrow_mut() = Some(new_source);
    }

    pub(super) fn handle_compute_diff(&self) {
        let imp = self.imp();
        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();

        let left_text = left_buf
            .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
            .to_string();
        let right_text = right_buf
            .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
            .to_string();

        let ignore_ws = imp.sw_ignore_whitespace.is_active();
        let patience = imp.pref_patience.is_active();
        let total_lines = count_lines(&left_text) + count_lines(&right_text);

        if total_lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_diff_count.set_label(tr!("Calculando...", "Calculating..."));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let summary = compute_diff_ext(&left_text, &right_text, patience, ignore_ws);
                glib::idle_add_once(move || {
                    handles
                        .into_inner()
                        .handle_worker_result(WorkerResult::DiffCalculated(summary));
                });
            });
        } else {
            let summary = compute_diff_ext(&left_text, &right_text, patience, ignore_ws);
            self.apply_diff_summary(summary);
        }
    }

    pub(super) fn apply_diff_summary(&self, summary: DiffSummary) {
        self.clear_diff_highlights();
        let imp = self.imp();

        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();

        // 1. Line-level highlights
        if let Some(tag_del) = imp.tag_deleted.get() {
            for h in &summary.left_highlights {
                if let Some(start_iter) = left_buf.iter_at_line(h.line_index as i32) {
                    let mut end_iter = start_iter;
                    if !end_iter.ends_line() {
                        end_iter.forward_to_line_end();
                    }
                    end_iter.forward_char();
                    left_buf.apply_tag(tag_del, &start_iter, &end_iter);
                }
            }
        }

        if let Some(tag_ins) = imp.tag_inserted.get() {
            for h in &summary.right_highlights {
                if let Some(start_iter) = right_buf.iter_at_line(h.line_index as i32) {
                    let mut end_iter = start_iter;
                    if !end_iter.ends_line() {
                        end_iter.forward_to_line_end();
                    }
                    end_iter.forward_char();
                    right_buf.apply_tag(tag_ins, &start_iter, &end_iter);
                }
            }
        }

        // 2. Inline word/token-level highlights & vertical accent stripes
        let left_marks: Vec<crate::ui::diff_editor::DiffLineMark> = summary
            .left_highlights
            .iter()
            .map(|h| crate::ui::diff_editor::DiffLineMark {
                line: h.line_index as i32,
                is_deletion: true,
            })
            .collect();
        imp.source_diff_left.set_diff_line_marks(left_marks);

        let right_marks: Vec<crate::ui::diff_editor::DiffLineMark> = summary
            .right_highlights
            .iter()
            .map(|h| crate::ui::diff_editor::DiffLineMark {
                line: h.line_index as i32,
                is_deletion: false,
            })
            .collect();
        imp.source_diff_right.set_diff_line_marks(right_marks);

        let left_diff_spans: Vec<crate::ui::diff_editor::DiffSpan> = summary
            .left_spans
            .iter()
            .map(|s| crate::ui::diff_editor::DiffSpan {
                line: s.line_index as i32,
                start_char: s.start_char as i32,
                end_char: s.end_char as i32,
                is_deletion: true,
            })
            .collect();
        imp.source_diff_left.set_diff_spans(left_diff_spans);

        let right_diff_spans: Vec<crate::ui::diff_editor::DiffSpan> = summary
            .right_spans
            .iter()
            .map(|s| crate::ui::diff_editor::DiffSpan {
                line: s.line_index as i32,
                start_char: s.start_char as i32,
                end_char: s.end_char as i32,
                is_deletion: false,
            })
            .collect();
        imp.source_diff_right.set_diff_spans(right_diff_spans);

        if let Some(tag_sdel) = imp.tag_span_deleted.get() {
            for s in &summary.left_spans {
                if let (Some(start_iter), Some(end_iter)) = (
                    left_buf.iter_at_line_offset(s.line_index as i32, s.start_char as i32),
                    left_buf.iter_at_line_offset(s.line_index as i32, s.end_char as i32),
                ) {
                    left_buf.apply_tag(tag_sdel, &start_iter, &end_iter);
                }
            }
        }

        if let Some(tag_sins) = imp.tag_span_inserted.get() {
            for s in &summary.right_spans {
                if let (Some(start_iter), Some(end_iter)) = (
                    right_buf.iter_at_line_offset(s.line_index as i32, s.start_char as i32),
                    right_buf.iter_at_line_offset(s.line_index as i32, s.end_char as i32),
                ) {
                    right_buf.apply_tag(tag_sins, &start_iter, &end_iter);
                }
            }
        }

        let count_label = if summary.hunk_count == 0 {
            tr!("Sin diferencias", "No differences").to_string()
        } else if summary.hunk_count == 1 {
            tr!("1 diferencia encontrada", "1 difference found").to_string()
        } else {
            format!("{} {}", summary.hunk_count, tr!("diferencias encontradas", "differences found"))
        };
        imp.lbl_diff_count.set_label(&count_label);

        let sim_pct = (summary.similarity_ratio * 100.0).round() as u32;
        imp.lbl_diff_similarity.set_label(&format!(
            "{}% {}",
            sim_pct,
            tr!("similitud", "similarity")
        ));

        imp.lbl_diff_removals
            .set_label(&format!("{} {}", summary.deletions, tr!("supresiones (-)", "deletions (-)")));
        imp.lbl_diff_additions
            .set_label(&format!("{} {}", summary.insertions, tr!("adiciones (+)", "additions (+)")));

        let left_txt = left_buf
            .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
            .to_string();
        let right_txt = right_buf
            .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
            .to_string();
        let fl = Self::detect_payload_format(&left_txt);
        let fr = Self::detect_payload_format(&right_txt);
        Self::set_buffer_language(
            &left_buf,
            match fl {
                "XML" => "xml",
                "PHP var_dump" | "Array raw" => "php",
                _ => "json",
            },
        );
        Self::set_buffer_language(
            &right_buf,
            match fr {
                "XML" => "xml",
                "PHP var_dump" | "Array raw" => "php",
                _ => "json",
            },
        );
        let fmt_label = match (left_txt.trim().is_empty(), right_txt.trim().is_empty()) {
            (true, true) => tr!("Texto", "Text").to_string(),
            (false, true) => fl.to_string(),
            (true, false) => fr.to_string(),
            (false, false) => {
                if fl == fr {
                    fl.to_string()
                } else {
                    format!("{} / {}", fl, fr)
                }
            }
        };
        imp.lbl_diff_encoding.set_label(&format!(
            "UTF-8  •  {}  •  {}",
            fmt_label,
            self.current_indent_style().label()
        ));

        imp.hunk_positions.replace(summary.hunk_positions);
        imp.last_unified_diff.replace(summary.unified_diff);
        imp.current_hunk_idx.set(0);
    }

    pub(super) fn detect_payload_format(text: &str) -> &'static str {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return "Texto";
        }
        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
            return "JSON";
        }
        if trimmed.starts_with('<') {
            return "XML";
        }
        if crate::core::php_serialize::is_php_serialized(trimmed)
            && parse_vardump(trimmed).is_ok()
        {
            return "PHP serialize()";
        }
        if parse_vardump(trimmed).is_ok() {
            if is_printr_raw(trimmed) {
                return "Array raw";
            }
            return "PHP var_dump";
        }
        "Texto"
    }

    pub(super) fn normalize_single_payload(text: &str, indent: IndentStyle) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(sorted_json) = sort_json_keys(trimmed, indent) {
            return Some(sorted_json);
        }
        if trimmed.starts_with('<') {
            if let Ok(fmt_xml) = format_xml_with_indent(trimmed, true, indent) {
                return Some(fmt_xml);
            }
        }
        if let Ok(fmt_php) = format_vardump_with_indent(trimmed, true, indent) {
            return Some(fmt_php);
        }
        None
    }

    pub(super) fn handle_normalize_diff(&self) {
        let imp = self.imp();
        let indent = self.current_indent_style();
        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();

        let left_text = left_buf
            .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
            .to_string();
        let right_text = right_buf
            .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
            .to_string();

        let mut had_error = false;
        if !left_text.trim().is_empty() {
            if let Some(norm_l) = Self::normalize_single_payload(&left_text, indent) {
                left_buf.set_text(&norm_l);
            } else {
                had_error = true;
            }
        }
        if !right_text.trim().is_empty() {
            if let Some(norm_r) = Self::normalize_single_payload(&right_text, indent) {
                right_buf.set_text(&norm_r);
            } else {
                had_error = true;
            }
        }
        self.handle_compute_diff();
        if had_error {
            imp.lbl_diff_sync.set_label(tr!(
                "⚠ Error de sintaxis al normalizar uno de los paneles",
                "⚠ Syntax error normalizing one of the panels"
            ));
        } else {
            imp.lbl_diff_sync.set_label(tr!(
                "✓ Paneles normalizados",
                "✓ Panels normalized"
            ));
        }
    }
}
