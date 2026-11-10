use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use sourceview5::subclass::prelude::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug)]
pub struct DiffSpan {
    pub line: i32,
    pub start_char: i32,
    pub end_char: i32,
    pub is_deletion: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DiffLineMark {
    pub line: i32,
    pub is_deletion: bool,
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct VarLensDiffView {
        pub spans: RefCell<Vec<DiffSpan>>,
        pub line_marks: RefCell<Vec<DiffLineMark>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VarLensDiffView {
        const NAME: &'static str = "VarLensDiffView";
        type Type = super::VarLensDiffView;
        type ParentType = sourceview5::View;
    }

    impl ObjectImpl for VarLensDiffView {}
    impl WidgetImpl for VarLensDiffView {}
    impl TextViewImpl for VarLensDiffView {
        fn snapshot_layer(&self, layer: gtk4::TextViewLayer, snapshot: gtk4::Snapshot) {
            if layer == gtk4::TextViewLayer::BelowText {
                let view = self.obj();
                let spans = self.spans.borrow();
                let marks = self.line_marks.borrow();
                let buf = view.buffer();
                let is_dark = libadwaita::StyleManager::default().is_dark();

                // Draw vertical 3.5px accent stripe on the left edge of changed lines
                for mark in marks.iter() {
                    if let Some(iter) = buf.iter_at_line(mark.line) {
                        let (y, h) = view.line_yrange(&iter);
                        let bar_color = if mark.is_deletion {
                            if is_dark {
                                gdk::RGBA::new(0.95, 0.25, 0.30, 0.95)
                            } else {
                                gdk::RGBA::new(0.85, 0.15, 0.20, 0.90)
                            }
                        } else if is_dark {
                            gdk::RGBA::new(0.15, 0.82, 0.50, 0.95)
                        } else {
                            gdk::RGBA::new(0.08, 0.65, 0.35, 0.90)
                        };
                        let rect = graphene::Rect::new(0.0, y as f32, 3.5, h.max(18) as f32);
                        snapshot.append_color(&bar_color, &rect);
                    }
                }

                for span in spans.iter() {
                    if let (Some(start_iter), Some(end_iter)) = (
                        buf.iter_at_line_offset(span.line, span.start_char),
                        buf.iter_at_line_offset(span.line, span.end_char),
                    ) {
                        let start_loc = view.iter_location(&start_iter);
                        let mut end_loc = view.iter_location(&end_iter);

                        // If end_loc wrapped to next line (e.g. at line end), clamp to start line
                        if end_loc.y() > start_loc.y() {
                            let mut line_end_iter = start_iter;
                            if !line_end_iter.ends_line() {
                                line_end_iter.forward_to_line_end();
                            }
                            end_loc = view.iter_location(&line_end_iter);
                        }

                        let end_x = if end_loc.x() > start_loc.x() {
                            end_loc.x()
                        } else {
                            start_loc.x() + start_loc.width().max(10)
                        };

                        let text_w = (end_x - start_loc.x()).max(start_loc.width());

                        // Generous horizontal and vertical breathing room for text visibility
                        let pad_x = 7.0f32;
                        let pad_y = 3.0f32;

                        let x = start_loc.x() as f32 - pad_x;
                        let y = start_loc.y() as f32 - pad_y;
                        let w = text_w as f32 + pad_x * 2.0;
                        let h = start_loc.height() as f32 + pad_y * 2.0;

                        if w > 0.0 && h > 0.0 {
                            let rect = graphene::Rect::new(x, y, w, h);
                            let rounded = gsk::RoundedRect::from_rect(rect, 6.0);

                            let bg_color = if is_dark {
                                if span.is_deletion {
                                    gdk::RGBA::new(0.78, 0.12, 0.16, 0.95)
                                } else {
                                    gdk::RGBA::new(0.02, 0.60, 0.38, 0.95)
                                }
                            } else {
                                if span.is_deletion {
                                    gdk::RGBA::new(0.85, 0.15, 0.20, 0.98) // Vibrant clean ruby red for light theme
                                } else {
                                    gdk::RGBA::new(0.08, 0.62, 0.35, 0.98) // Vibrant clean emerald for light theme
                                }
                            };

                            snapshot.push_rounded_clip(&rounded);
                            snapshot.append_color(&bg_color, &rect);
                            snapshot.pop();
                        }
                    }
                }
            }
            self.parent_snapshot_layer(layer, snapshot);
        }
    }
    impl ViewImpl for VarLensDiffView {}
}

glib::wrapper! {
    pub struct VarLensDiffView(ObjectSubclass<imp::VarLensDiffView>)
        @extends sourceview5::View, gtk4::TextView, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Scrollable;
}

impl Default for VarLensDiffView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl VarLensDiffView {
    pub fn ensure_type() {
        use glib::types::StaticType;
        Self::static_type();
    }

    pub fn set_diff_spans(&self, spans: Vec<DiffSpan>) {
        let imp = self.imp();
        *imp.spans.borrow_mut() = spans;
        self.queue_draw();
    }

    pub fn set_diff_line_marks(&self, marks: Vec<DiffLineMark>) {
        let imp = self.imp();
        *imp.line_marks.borrow_mut() = marks;
        self.queue_draw();
    }

    pub fn clear_diff_spans(&self) {
        let imp = self.imp();
        imp.spans.borrow_mut().clear();
        imp.line_marks.borrow_mut().clear();
        self.queue_draw();
    }
}
