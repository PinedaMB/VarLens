use crate::core::folding::{find_fold_regions, FoldRegion};
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct FoldingController {
    view: sourceview5::View,
    renderer: sourceview5::GutterRendererText,
    regions: Rc<RefCell<Vec<FoldRegion>>>,
    folded_lines: Rc<RefCell<HashSet<usize>>>,
}

impl FoldingController {
    pub fn new(view: sourceview5::View) -> Self {
        let gutter = sourceview5::prelude::ViewExt::gutter(&view, gtk4::TextWindowType::Left);
        let renderer = sourceview5::GutterRendererText::new();
        renderer.set_xpad(2);
        renderer.set_ypad(0);
        renderer.set_alignment_mode(sourceview5::GutterRendererAlignmentMode::First);

        let regions: Rc<RefCell<Vec<FoldRegion>>> = Rc::new(RefCell::new(Vec::new()));
        let folded_lines = Rc::new(RefCell::new(HashSet::new()));

        let reg_query = regions.clone();
        let fold_query = folded_lines.clone();
        renderer.connect_query_data(move |ren, _obj, line| {
            let line = line as usize;
            let is_region_start = reg_query.borrow().iter().any(|r| r.start_line == line);
            if is_region_start {
                if fold_query.borrow().contains(&line) {
                    ren.set_markup("<span size='small' foreground='#3584e4'>▶</span>");
                } else {
                    ren.set_markup("<span size='small' alpha='60%'>▼</span>");
                }
            } else {
                ren.set_text("");
            }
        });

        renderer.connect_query_activatable(|_ren, _iter, _area| true);

        let view_weak = view.downgrade();
        let reg_act = regions.clone();
        let fold_act = folded_lines.clone();
        let ren_act = renderer.clone();

        renderer.connect_activate(move |_ren, iter, _area, _button, _state, _n_presses| {
            let line = iter.line() as usize;
            if let Some(region) = reg_act.borrow().iter().find(|r| r.start_line == line).cloned() {
                if let Some(view) = view_weak.upgrade() {
                    let buf = view.buffer();
                    let tag_name = format!("fold-line-{}", line);
                    let is_folded = fold_act.borrow().contains(&line);

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

                    // Fold range is from the end of the opening line (or start of next line)
                    // up to the closing bracket line.
                    if let (Some(start_iter), Some(end_iter)) = (
                        buf.iter_at_line((line + 1) as i32),
                        buf.iter_at_line(region.end_line as i32),
                    ) {
                        if is_folded {
                            buf.remove_tag(&tag, &start_iter, &end_iter);
                            fold_act.borrow_mut().remove(&line);
                        } else {
                            buf.apply_tag(&tag, &start_iter, &end_iter);
                            fold_act.borrow_mut().insert(line);
                        }
                    }
                    ren_act.queue_draw();
                }
            }
        });

        // Insert into gutter right next to the line numbers
        gutter.insert(&renderer, -40);

        let controller = Self {
            view,
            renderer,
            regions,
            folded_lines,
        };

        controller.update_regions();
        controller
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
                    if let (Some(start_iter), Some(end_iter)) = (
                        buf.iter_at_line((line + 1) as i32),
                        buf.iter_at_line(reg.end_line as i32),
                    ) {
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
        self.renderer.queue_draw();
    }
}
