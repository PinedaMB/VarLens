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

        // 1. Custom Line Numbers Renderer (with ample width and padding)
        let renderer_lines = sourceview5::GutterRendererText::new();
        renderer_lines.set_size_request(38, -1);
        renderer_lines.set_xpad(8);
        renderer_lines.set_ypad(0);
        renderer_lines.set_xalign(1.0); // right-align line numbers
        renderer_lines.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::First);

        let reg_lines = regions.clone();
        let fold_lines = folded_lines.clone();
        renderer_lines.connect_query_data(move |ren, _obj, line| {
            let line = line as usize;
            let hidden = {
                let folds = fold_lines.borrow();
                let regs = reg_lines.borrow();
                is_line_folded_away(line, &folds, &regs)
            };

            if hidden {
                ren.set_text("");
            } else {
                ren.set_markup(&format!(
                    "<span font_desc='monospace' foreground='#75777e'>{}</span>",
                    line + 1
                ));
            }
        });

        // 2. Fold Arrows Renderer (dedicated column with hand pointer cursor and padding)
        let renderer_fold = sourceview5::GutterRendererText::new();
        renderer_fold.set_size_request(22, -1);
        renderer_fold.set_xpad(5);
        renderer_fold.set_ypad(0);
        renderer_fold.set_xalign(0.5); // centered arrow
        renderer_fold.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::First);
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
                ren.set_markup("<span size='small' foreground='#3584e4'>▶</span>");
            } else if regs.iter().any(|r| r.start_line == line) {
                ren.set_markup("<span size='small' alpha='45%'>▼</span>");
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

        // Direct click gestures on the fold renderer, the line renderer, and gutter
        Self::attach_click_gesture(&renderer_fold, &view, &regions, &folded_lines, &renderer_lines, &renderer_fold);
        Self::attach_click_gesture(&renderer_lines, &view, &regions, &folded_lines, &renderer_lines, &renderer_fold);
        Self::attach_click_gesture(&gutter, &view, &regions, &folded_lines, &renderer_lines, &renderer_fold);

        // Insert into gutter: line numbers at -30 (left), fold arrows at -20 (right, beside code)
        gutter.insert(&renderer_lines, -30);
        gutter.insert(&renderer_fold, -20);

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

            if let (Some(mut start_iter), Some(end_iter)) = (
                buf.iter_at_line(region.start_line as i32),
                buf.iter_at_line(region.end_line as i32),
            ) {
                // Keep the opening line/bracket visible, collapse lines inside the block
                start_iter.forward_to_line_end();

                if is_folded {
                    buf.remove_tag(&tag, &start_iter, &end_iter);
                    folded_lines.borrow_mut().remove(&line);
                } else {
                    buf.apply_tag(&tag, &start_iter, &end_iter);
                    folded_lines.borrow_mut().insert(line);
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
            if let Some(reg) = new_regions.iter().find(|r| r.start_line == line) {
                let tag_name = format!("fold-line-{}", line);
                if let Some(tag) = buf.tag_table().lookup(&tag_name) {
                    if let (Some(mut start_iter), Some(end_iter)) = (
                        buf.iter_at_line(reg.start_line as i32),
                        buf.iter_at_line(reg.end_line as i32),
                    ) {
                        start_iter.forward_to_line_end();
                        buf.apply_tag(&tag, &start_iter, &end_iter);
                    }
                }
                true
            } else {
                let tag_name = format!("fold-line-{}", line);
                if let Some(tag) = buf.tag_table().lookup(&tag_name) {
                    buf.remove_tag(&tag, &buf.start_iter(), &buf.end_iter());
                }
                false
            }
        });

        *self.regions.borrow_mut() = new_regions;
        self.renderer_lines.queue_draw();
        self.renderer_fold.queue_draw();
    }
}
