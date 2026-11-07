use crate::core::diff::{compute_diff, DiffSummary};
use crate::ui::editor::EditorView;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
#[allow(dead_code)]
pub struct DiffView {
    container: gtk4::Box,
    paned: gtk4::Paned,
    left_editor: EditorView,
    right_editor: EditorView,
    stats_label: gtk4::Label,
    tag_deleted: gtk4::TextTag,
    tag_inserted: gtk4::TextTag,
    patience_switch: gtk4::CheckButton,
    last_summary: Rc<Cell<Option<DiffSummary>>>,
}

#[allow(dead_code)]
impl DiffView {
    pub fn new() -> Self {
        let left_editor = EditorView::new(Some("json"));
        let right_editor = EditorView::new(Some("json"));

        // Setup tags for insertions (green) and deletions (red)
        let tag_deleted = gtk4::TextTag::builder()
            .name("diff-deleted")
            .background_rgba(&gdk::RGBA::new(0.88, 0.11, 0.14, 0.28)) // Red / .error
            .paragraph_background_rgba(&gdk::RGBA::new(0.88, 0.11, 0.14, 0.20))
            .build();

        let tag_inserted = gtk4::TextTag::builder()
            .name("diff-inserted")
            .background_rgba(&gdk::RGBA::new(0.18, 0.76, 0.49, 0.28)) // Green / .success
            .paragraph_background_rgba(&gdk::RGBA::new(0.18, 0.76, 0.49, 0.20))
            .build();

        left_editor.buffer().tag_table().add(&tag_deleted);
        right_editor.buffer().tag_table().add(&tag_inserted);

        // Synchronize scrolling between left and right views
        Self::sync_scrolling(left_editor.widget(), right_editor.widget());

        // Header controls for diff view
        let left_label = gtk4::Label::builder()
            .label("<b>Original</b>")
            .use_markup(true)
            .halign(gtk4::Align::Center)
            .build();

        let right_label = gtk4::Label::builder()
            .label("<b>Modificado</b>")
            .use_markup(true)
            .halign(gtk4::Align::Center)
            .build();

        let left_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .vexpand(true)
            .build();
        left_box.append(&left_label);
        left_box.append(left_editor.widget());

        let right_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .vexpand(true)
            .build();
        right_box.append(&right_label);
        right_box.append(right_editor.widget());

        let paned = gtk4::Paned::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .start_child(&left_box)
            .end_child(&right_box)
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(false)
            .shrink_end_child(false)
            .hexpand(true)
            .vexpand(true)
            .build();

        // Diff toolbar below or above
        let diff_toolbar = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let patience_switch = gtk4::CheckButton::builder()
            .label("Algoritmo Patience")
            .tooltip_text("Usa el algoritmo Patience en lugar de Myers para diffs más legibles")
            .active(false)
            .build();

        let stats_label = gtk4::Label::builder()
            .label("Sin diferencias calculadas aún")
            .hexpand(true)
            .halign(gtk4::Align::End)
            .css_classes(["dim-label"])
            .build();

        diff_toolbar.append(&patience_switch);
        diff_toolbar.append(&stats_label);

        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(6)
            .hexpand(true)
            .vexpand(true)
            .build();

        container.append(&diff_toolbar);
        container.append(&paned);

        let last_summary = Rc::new(Cell::new(None));

        let view = Self {
            container,
            paned,
            left_editor,
            right_editor,
            stats_label,
            tag_deleted,
            tag_inserted,
            patience_switch,
            last_summary,
        };

        // Recompute on switch toggle
        let view_clone = view.clone();
        view.patience_switch.connect_toggled(move |_| {
            view_clone.update_diff();
        });

        view
    }

    /// Sets up bidirectional vertical scroll synchronization.
    fn sync_scrolling(left: &gtk4::ScrolledWindow, right: &gtk4::ScrolledWindow) {
        let left_adj = left.vadjustment();
        let right_adj = right.vadjustment();

        let updating = Rc::new(Cell::new(false));

        let up_clone1 = updating.clone();
        let right_adj_clone = right_adj.clone();
        left_adj.connect_value_changed(move |adj| {
            if up_clone1.get() {
                return;
            }
            up_clone1.set(true);
            right_adj_clone.set_value(adj.value());
            up_clone1.set(false);
        });

        let up_clone2 = updating.clone();
        let left_adj_clone = left_adj.clone();
        right_adj.connect_value_changed(move |adj| {
            if up_clone2.get() {
                return;
            }
            up_clone2.set(true);
            left_adj_clone.set_value(adj.value());
            up_clone2.set(false);
        });
    }

    /// Clears any applied highlight tags.
    pub fn clear_highlights(&self) {
        let left_buf = self.left_editor.buffer();
        left_buf.remove_tag(&self.tag_deleted, &left_buf.start_iter(), &left_buf.end_iter());

        let right_buf = self.right_editor.buffer();
        right_buf.remove_tag(&self.tag_inserted, &right_buf.start_iter(), &right_buf.end_iter());
    }

    /// Applies a computed DiffSummary to the left and right buffers.
    pub fn apply_diff_summary(&self, summary: DiffSummary) {
        self.clear_highlights();

        let left_buf = self.left_editor.buffer();
        for h in &summary.left_highlights {
            if let Some(start_iter) = left_buf.iter_at_line(h.line_index as i32) {
                let mut end_iter = start_iter;
                if !end_iter.ends_line() {
                    end_iter.forward_to_line_end();
                }
                end_iter.forward_char();
                left_buf.apply_tag(&self.tag_deleted, &start_iter, &end_iter);
            }
        }

        let right_buf = self.right_editor.buffer();
        for h in &summary.right_highlights {
            if let Some(start_iter) = right_buf.iter_at_line(h.line_index as i32) {
                let mut end_iter = start_iter;
                if !end_iter.ends_line() {
                    end_iter.forward_to_line_end();
                }
                end_iter.forward_char();
                right_buf.apply_tag(&self.tag_inserted, &start_iter, &end_iter);
            }
        }

        let stats_text = format!(
            "+{} inserciones | -{} supresiones | {:.1}% similitud",
            summary.insertions,
            summary.deletions,
            summary.similarity_ratio * 100.0
        );
        self.stats_label.set_label(&stats_text);
        self.stats_label.remove_css_class("dim-label");

        self.last_summary.set(Some(summary));
    }

    /// Computes and applies diff synchronously.
    pub fn update_diff(&self) {
        let left_text = self.left_editor.text();
        let right_text = self.right_editor.text();
        let patience = self.patience_switch.is_active();

        let summary = compute_diff(&left_text, &right_text, patience);
        self.apply_diff_summary(summary);
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn left_editor(&self) -> &EditorView {
        &self.left_editor
    }

    pub fn right_editor(&self) -> &EditorView {
        &self.right_editor
    }

    pub fn is_patience_active(&self) -> bool {
        self.patience_switch.is_active()
    }
}
