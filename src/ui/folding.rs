use crate::core::folding::{find_fold_regions, FoldRegion};
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// Checks if a buffer line is hidden inside any currently folded block.
fn is_line_folded_away(
    line: usize,
    folded_lines: &HashSet<usize>,
    regions: &[FoldRegion],
) -> bool {
    for &f_start in folded_lines {
        if let Some(r) = regions.iter().find(|r| r.start_line == f_start) {
            if line > r.start_line && line < r.end_line {
                return true;
            }
        }
    }
    false
}

#[derive(Clone, Debug)]
pub struct FoldingController {
    view: sourceview5::View,
    renderer_lines: sourceview5::GutterRendererText,
    renderer_fold: sourceview5::GutterRendererText,
    regions: Rc<RefCell<Vec<FoldRegion>>>,
    folded_lines: Rc<RefCell<HashSet<usize>>>,
}

impl FoldingController {
    pub fn new(view: sourceview5::View) -> Self {
        // Disable native line numbers and marks to avoid duplicated/colliding rendering
        view.set_show_line_numbers(false);
        view.set_show_line_marks(false);
        // Add comfortable left margin between gutter and editor text
        view.set_left_margin(14);

        let gutter = sourceview5::prelude::ViewExt::gutter(&view, gtk4::TextWindowType::Left);

        let regions: Rc<RefCell<Vec<FoldRegion>>> = Rc::new(RefCell::new(Vec::new()));
        let folded_lines: Rc<RefCell<HashSet<usize>>> = Rc::new(RefCell::new(HashSet::new()));

        // 1. Custom Line Numbers Renderer (with ample width and vertical breathing room)
        let renderer_lines = sourceview5::GutterRendererText::new();
        renderer_lines.set_size_request(44, -1);
        renderer_lines.set_xpad(8);
        renderer_lines.set_ypad(2);
        renderer_lines.set_xalign(1.0); // right-align line numbers
        renderer_lines.set_yalign(0.5); // vertically center to prevent clipping at the top
        renderer_lines.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::Cell);

        let reg_lines = regions.clone();
        let fold_lines = folded_lines.clone();
        let view_for_lines = view.downgrade();
        renderer_lines.connect_query_data(move |ren, _obj, line| {
            let line = line as usize;
            let (hidden, is_fold_header) = {
                let folds = fold_lines.borrow();
                let regs = reg_lines.borrow();
                (
                    is_line_folded_away(line, &folds, &regs),
                    folds.contains(&line),
                )
            };

            if hidden {
                ren.set_text("");
            } else {
                let is_cursor_line = view_for_lines
                    .upgrade()
                    .map(|v| {
                        let buf = v.buffer();
                        let iter = buf.iter_at_offset(buf.cursor_position());
                        iter.line() as usize == line
                    })
                    .unwrap_or(false);

                if is_fold_header {
                    ren.set_markup(&format!(
                        "<span font_desc='Fira Code, monospace 10.5pt' weight='bold' foreground='#3584e4'>{}</span>",
                        line + 1
                    ));
                } else if is_cursor_line {
                    let fg = if libadwaita::StyleManager::default().is_dark() {
                        "#e6edf3"
                    } else {
                        "#1f2328"
                    };
                    ren.set_markup(&format!(
                        "<span font_desc='Fira Code, monospace 10.5pt' weight='bold' foreground='{}'>{}</span>",
                        fg,
                        line + 1
                    ));
                } else {
                    ren.set_markup(&format!(
                        "<span font_desc='Fira Code, monospace 10.5pt' foreground='#75777e'>{}</span>",
                        line + 1
                    ));
                }
            }
        });

        // 2. Fold Arrows Renderer (dedicated column with hand pointer cursor and padding)
        let renderer_fold = sourceview5::GutterRendererText::new();
        renderer_fold.set_size_request(22, -1);
        renderer_fold.set_xpad(5);
        renderer_fold.set_ypad(2);
        renderer_fold.set_xalign(0.5); // centered arrow
        renderer_fold.set_yalign(0.5);
        renderer_fold.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::Cell);
        renderer_fold.set_cursor_from_name(Some("pointer"));

        let reg_fold = regions.clone();
        let fold_fold = folded_lines.clone();
        renderer_fold.connect_query_data(move |ren, _obj, line| {
            let line = line as usize;
            let folds = fold_fold.borrow();
            let regs = reg_fold.borrow();

            if is_line_folded_away(line, &folds, &regs) {
                ren.set_text("");
            } else if folds.contains(&line) {
                ren.set_markup("<span size='small' weight='bold' foreground='#3584e4'>▶</span>");
            } else if regs.iter().any(|r| r.start_line == line) {
                ren.set_markup("<span size='small' alpha='55%'>▼</span>");
            } else {
                ren.set_text("");
            }
        });

        renderer_fold.connect_query_activatable(|_ren, _iter, _area| true);

        // Native gutter renderer activate signal
        let view_weak = view.downgrade();
        let reg_act = regions.clone();
        let fold_act = folded_lines.clone();
        let ren_lines_act = renderer_lines.clone();
        let ren_fold_act = renderer_fold.clone();
        renderer_fold.connect_activate(move |_ren, iter, _area, _button, _state, _n_presses| {
            let line = iter.line() as usize;
            if let Some(view) = view_weak.upgrade() {
                if Self::toggle_fold_at_line(&view, &reg_act, &fold_act, line) {
                    ren_lines_act.queue_draw();
                    ren_fold_act.queue_draw();
                    view.queue_draw();
                }
            }
        });

        // Direct click gesture on the line numbers renderer so clicking line numbers also toggles fold
        Self::attach_click_gesture(&renderer_lines, &view, &regions, &folded_lines, &renderer_lines, &renderer_fold);

        // Insert into gutter: line numbers at -30 (left), fold arrows at -20 (right, beside code)
        gutter.insert(&renderer_lines, -30);
        gutter.insert(&renderer_fold, -20);

        let ren_lines_cur = renderer_lines.clone();
        view.buffer().connect_cursor_position_notify(move |_| {
            ren_lines_cur.queue_draw();
        });

        let controller = Self {
            view,
            renderer_lines,
            renderer_fold,
            regions,
            folded_lines,
        };

        controller.update_regions();
        controller
    }

    /// Attaches a click gesture controller that maps clicks from any gutter widget to the corresponding buffer line.
    fn attach_click_gesture<W: IsA<gtk4::Widget>>(
        widget: &W,
        view: &sourceview5::View,
        regions: &Rc<RefCell<Vec<FoldRegion>>>,
        folded_lines: &Rc<RefCell<HashSet<usize>>>,
        ren_lines: &sourceview5::GutterRendererText,
        ren_fold: &sourceview5::GutterRendererText,
    ) {
        let view_weak = view.downgrade();
        let reg_cl = regions.clone();
        let fold_cl = folded_lines.clone();
        let ren_l_cl = ren_lines.clone();
        let ren_f_cl = ren_fold.clone();
        let target_widget = widget.clone().upcast::<gtk4::Widget>();

        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |g, _n_press, _x, y| {
            if let Some(view) = view_weak.upgrade() {
                let win_y = target_widget
                    .compute_point(&view, &gtk4::graphene::Point::new(0.0, y as f32))
                    .map(|p| p.y() as f64)
                    .unwrap_or(y);

                let (_, buf_y) =
                    view.window_to_buffer_coords(gtk4::TextWindowType::Widget, 0, win_y as i32);
                let (iter, _top) = view.line_at_y(buf_y);
                let line = iter.line() as usize;

                if Self::toggle_fold_at_line(&view, &reg_cl, &fold_cl, line) {
                    g.set_state(gtk4::EventSequenceState::Claimed);
                    ren_l_cl.queue_draw();
                    ren_f_cl.queue_draw();
                    view.queue_draw();
                }
            }
        });
        widget.add_controller(gesture);
    }

    /// Toggles collapse / expand for a given line. Returns true if line was a foldable start line.
    fn toggle_fold_at_line(
        view: &sourceview5::View,
        regions: &Rc<RefCell<Vec<FoldRegion>>>,
        folded_lines: &Rc<RefCell<HashSet<usize>>>,
        line: usize,
    ) -> bool {
        let region_opt = regions.borrow().iter().find(|r| r.start_line == line).cloned();
        if let Some(region) = region_opt {
            let buf = view.buffer();
            let tag_name = format!("fold-line-{}", line);
            let is_folded = folded_lines.borrow().contains(&line);

            let tag = if let Some(t) = buf.tag_table().lookup(&tag_name) {
                t
            } else {
                let t = gtk4::TextTag::builder()
                    .name(&tag_name)
                    .invisible(true)
                    .build();
                buf.tag_table().add(&t);
                t
            };

            let header_tag = if let Some(ht) = buf.tag_table().lookup("fold-header-highlight") {
                ht
            } else {
                let ht = gtk4::TextTag::builder()
                    .name("fold-header-highlight")
                    .paragraph_background_rgba(&gtk4::gdk::RGBA::new(0.21, 0.52, 0.89, 0.12))
                    .build();
                buf.tag_table().add(&ht);
                ht
            };

            if let (Some(mut start_iter), Some(mut end_iter)) = (
                buf.iter_at_line(region.start_line as i32),
                buf.iter_at_line(region.end_line as i32),
            ) {
                let header_start = start_iter;
                let mut header_end = start_iter;
                if !header_end.ends_line() {
                    header_end.forward_to_line_end();
                }

                // Keep the opening line visible, collapse the inner lines (including the newline
                // right before the closing line so the closing bracket stays on its own line
                // without merging lines and invalidating Pango/GtkTextBTree byte offsets).
                if start_iter.forward_line() && end_iter.backward_char() && start_iter < end_iter {
                    if is_folded {
                        buf.remove_tag(&tag, &start_iter, &end_iter);
                        buf.remove_tag(&header_tag, &header_start, &header_end);
                        folded_lines.borrow_mut().remove(&line);
                    } else {
                        buf.apply_tag(&tag, &start_iter, &end_iter);
                        buf.apply_tag(&header_tag, &header_start, &header_end);
                        folded_lines.borrow_mut().insert(line);
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Scans the buffer and updates foldable regions. Removes folding tags if regions changed.
    pub fn update_regions(&self) {
        let buf = self.view.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();

        let new_regions = find_fold_regions(&text);

        // Clear any old folds that no longer match
        let mut folded = self.folded_lines.borrow_mut();
        folded.retain(|&line| {
            let tag_name = format!("fold-line-{}", line);
            if let Some(tag) = buf.tag_table().lookup(&tag_name) {
                buf.remove_tag(&tag, &buf.start_iter(), &buf.end_iter());
            }
            if let Some(reg) = new_regions.iter().find(|r| r.start_line == line) {
                if let Some(tag) = buf.tag_table().lookup(&tag_name) {
                    if let (Some(mut start_iter), Some(mut end_iter)) = (
                        buf.iter_at_line(reg.start_line as i32),
                        buf.iter_at_line(reg.end_line as i32),
                    ) {
                        if start_iter.forward_line()
                            && end_iter.backward_char()
                            && start_iter < end_iter
                        {
                            buf.apply_tag(&tag, &start_iter, &end_iter);
                        }
                    }
                }
                true
            } else {
                false
            }
        });

        *self.regions.borrow_mut() = new_regions;
        self.renderer_lines.queue_draw();
        self.renderer_fold.queue_draw();
    }

    /// Shows or hides the custom line number gutter column.
    pub fn set_line_numbers_visible(&self, visible: bool) {
        self.renderer_lines.set_visible(visible);
        self.renderer_lines.queue_draw();
    }
}
