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

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct VarLensDiffView {
        pub spans: RefCell<Vec<DiffSpan>>,
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
                let buf = view.buffer();

                for span in spans.iter() {
                    if let (Some(start_iter), Some(end_iter)) = (
                        buf.iter_at_line_offset(span.line, span.start_char),
                        buf.iter_at_line_offset(span.line, span.end_char),
                    ) {
                        let start_loc = view.iter_location(&start_iter);
                        let end_loc = view.iter_location(&end_iter);

                        let x = (start_loc.x() - 3) as f32;
                        let y = (start_loc.y() + 1) as f32;
                        let w = ((end_loc.x() - start_loc.x()) + 6).max(0) as f32;
                        let h = (start_loc.height() - 2).max(0) as f32;

                        if w > 0.0 && h > 0.0 {
                            let rect = graphene::Rect::new(x, y, w, h);
                            let rounded = gsk::RoundedRect::from_rect(rect, 5.0);

                            let bg_color = if span.is_deletion {
                                gdk::RGBA::new(0.72, 0.11, 0.15, 0.95)
                            } else {
                                gdk::RGBA::new(0.0, 0.53, 0.35, 0.95)
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

    pub fn clear_diff_spans(&self) {
        let imp = self.imp();
        imp.spans.borrow_mut().clear();
        self.queue_draw();
    }
}
