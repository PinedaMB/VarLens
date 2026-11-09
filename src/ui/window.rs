use crate::core::diff::{compute_diff_ext, DiffSummary};
use crate::core::json::{
    analyze_json, count_lines, format_json_with_indent, IndentStyle,
};
use crate::core::vardump::{parse_vardump, vardump_to_json_string};
use crate::core::xml::{analyze_xml, format_xml_with_indent, xml_to_json_string};
use crate::ui::editor::EditorView;
use crate::ui::folding::FoldingController;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use sourceview5::prelude::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const ASYNC_LINE_THRESHOLD: usize = 1000;

enum WorkerResult {
    InspectorFormatted { text: String, message: String },
    DiffCalculated(DiffSummary),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileTarget {
    Active,
    DiffLeft,
    DiffRight,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk4::CompositeTemplate)]
    #[template(file = "window.ui")]
    pub struct VarDiffWindow {
        #[template_child]
        pub main_stack: TemplateChild<adw::ViewStack>,

        // Diff page widgets
        #[template_child]
        pub diff_paned: TemplateChild<gtk4::Paned>,
        #[template_child]
        pub source_diff_left: TemplateChild<crate::ui::diff_editor::VarLensDiffView>,
        #[template_child]
        pub source_diff_right: TemplateChild<crate::ui::diff_editor::VarLensDiffView>,
        #[template_child]
        pub lbl_left_file: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_right_file: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_count: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_detect_mode: TemplateChild<gtk4::Label>,
        #[template_child]
        pub sw_ignore_whitespace: TemplateChild<gtk4::Switch>,
        #[template_child]
        pub lbl_diff_removals: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_additions: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_cursor: TemplateChild<gtk4::Label>,

        // Inspector page widgets
        #[template_child]
        pub source_inspector: TemplateChild<sourceview5::View>,
        #[template_child]
        pub source_map: TemplateChild<sourceview5::Map>,
        #[template_child]
        pub lbl_syntax_valid: TemplateChild<gtk4::Label>,
        #[template_child]
        pub dd_indent: TemplateChild<gtk4::DropDown>,
        #[template_child]
        pub btn_paste_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_copy_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_format_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_vardump_to_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_minify_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_mode_json: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_vardump: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_raw: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_xml: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub lbl_inspector_file: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_node_count: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_payload_size: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_tree_stats: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_rfc_badge: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_inspector_status: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_indent_info: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_inspector_cursor: TemplateChild<gtk4::Label>,

        // Preferences widgets
        #[template_child]
        pub pref_line_numbers: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_minimap: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_highlight_line: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_word_wrap: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_patience: TemplateChild<adw::SwitchRow>,

        // Global footer
        #[template_child]
        pub lbl_footer_diff_badge: TemplateChild<gtk4::Label>,

        // Internal state & tags
        pub tag_deleted: OnceCell<gtk4::TextTag>,
        pub tag_inserted: OnceCell<gtk4::TextTag>,
        pub tag_span_deleted: OnceCell<gtk4::TextTag>,
        pub tag_span_inserted: OnceCell<gtk4::TextTag>,
        pub hunk_positions: RefCell<Vec<(usize, usize)>>,
        pub current_hunk_idx: Cell<usize>,
        pub diff_debounce_source: RefCell<Option<glib::SourceId>>,
        pub fold_inspector: OnceCell<FoldingController>,
        pub fold_diff_left: OnceCell<FoldingController>,
        pub fold_diff_right: OnceCell<FoldingController>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VarDiffWindow {
        const NAME: &'static str = "VarDiffWindow";
        type Type = super::VarDiffWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            sourceview5::View::ensure_type();
            sourceview5::Map::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for VarDiffWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_window();
        }
    }

    impl WidgetImpl for VarDiffWindow {}
    impl WindowImpl for VarDiffWindow {}
    impl ApplicationWindowImpl for VarDiffWindow {}
    impl AdwApplicationWindowImpl for VarDiffWindow {}
}

glib::wrapper! {
    pub struct VarDiffWindow(ObjectSubclass<imp::VarDiffWindow>)
        @extends adw::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl VarDiffWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder()
            .property("application", app)
            .build()
    }

    fn setup_window(&self) {
        let imp = self.imp();

        // 1. Setup Line & Inline Word Diff Tags
        let tag_del = gtk4::TextTag::builder()
            .name("diff-deleted")
            .paragraph_background_rgba(&gdk::RGBA::new(0.92, 0.15, 0.18, 0.20))
            .build();

        let tag_ins = gtk4::TextTag::builder()
            .name("diff-inserted")
            .paragraph_background_rgba(&gdk::RGBA::new(0.0, 0.62, 0.35, 0.20))
            .build();

        let tag_span_del = gtk4::TextTag::builder()
            .name("diff-span-deleted")
            .foreground_rgba(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0))
            .weight(700)
            .build();

        let tag_span_ins = gtk4::TextTag::builder()
            .name("diff-span-inserted")
            .foreground_rgba(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0))
            .weight(700)
            .build();

        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();
        let inspector_buf = imp.source_inspector.buffer();

        left_buf.tag_table().add(&tag_del);
        left_buf.tag_table().add(&tag_span_del);
        right_buf.tag_table().add(&tag_ins);
        right_buf.tag_table().add(&tag_span_ins);

        imp.tag_deleted.set(tag_del).ok();
        imp.tag_inserted.set(tag_ins).ok();
        imp.tag_span_deleted.set(tag_span_del).ok();
        imp.tag_span_inserted.set(tag_span_ins).ok();

        // 2. Configure Style Schemes & Dark/Light Mode Adaptivity
        let style_manager = adw::StyleManager::default();
        Self::apply_schemes(
            &left_buf,
            &right_buf,
            &inspector_buf,
            style_manager.is_dark(),
        );

        let lb_clone = left_buf.clone();
        let rb_clone = right_buf.clone();
        let ib_clone = inspector_buf.clone();
        style_manager.connect_dark_notify(move |sm| {
            Self::apply_schemes(&lb_clone, &rb_clone, &ib_clone, sm.is_dark());
        });

        // 3. Set Initial Syntax Highlighting Languages
        Self::set_buffer_language(&left_buf, "json");
        Self::set_buffer_language(&right_buf, "json");
        Self::set_buffer_language(&inspector_buf, "json");

        // 4. Synchronize Scroll in Diff Paned
        if let (Some(left_parent), Some(right_parent)) = (
            imp.source_diff_left.parent(),
            imp.source_diff_right.parent(),
        ) {
            if let (Ok(left_scroll), Ok(right_scroll)) = (
                left_parent.downcast::<gtk4::ScrolledWindow>(),
                right_parent.downcast::<gtk4::ScrolledWindow>(),
            ) {
                Self::sync_scrolling(&left_scroll, &right_scroll);
            }
        }

        // 5. Connect Signals (Cursor tracking, live metrics, switches, dropdowns, preferences)
        self.setup_signals();

        // 6. Setup Code Folding Gutters
        let fold_ins = FoldingController::new(imp.source_inspector.get());
        let fold_dl = FoldingController::new(imp.source_diff_left.get().upcast());
        let fold_dr = FoldingController::new(imp.source_diff_right.get().upcast());
        imp.fold_inspector.set(fold_ins).ok();
        imp.fold_diff_left.set(fold_dl).ok();
        imp.fold_diff_right.set(fold_dr).ok();

        // 7. Setup Minimap Drag and Scroll
        self.setup_minimap();

        // 8. Register Window Actions & Shortcuts
        self.setup_actions();
        self.setup_shortcuts();

        // 9. Start with clean editors and initialize metrics
        self.update_inspector_metrics();
    }

    fn set_buffer_language(buf: &gtk4::TextBuffer, lang_id: &str) {
        let lang_mgr = sourceview5::LanguageManager::default();
        if let Some(sbuf) = buf.downcast_ref::<sourceview5::Buffer>() {
            let lang = lang_mgr.language(lang_id);
            sbuf.set_language(lang.as_ref());
        }
    }

    fn apply_schemes(
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

    fn sync_scrolling(left: &gtk4::ScrolledWindow, right: &gtk4::ScrolledWindow) {
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

    fn current_indent_style(&self) -> IndentStyle {
        match self.imp().dd_indent.selected() {
            1 => IndentStyle::Spaces4,
            2 => IndentStyle::Tabs,
            _ => IndentStyle::Spaces2,
        }
    }

    fn setup_signals(&self) {
        let imp = self.imp();

        // Ignore whitespace switch in Diff toolbar
        let win_ws = self.clone();
        imp.sw_ignore_whitespace.connect_state_set(move |_, _| {
            let win_clone = win_ws.clone();
            glib::idle_add_local_once(move || {
                win_clone.handle_compute_diff();
            });
            glib::Propagation::Proceed
        });

        // Indentation dropdown in Inspector toolbar
        let win_ind = self.clone();
        imp.dd_indent.connect_selected_notify(move |_| {
            let style = win_ind.current_indent_style();
            let imp = win_ind.imp();
            let tab_w = match style {
                IndentStyle::Spaces4 => 4,
                _ => 2,
            };
            imp.source_inspector.set_tab_width(tab_w);
            imp.source_inspector
                .set_insert_spaces_instead_of_tabs(style != IndentStyle::Tabs);
            imp.lbl_indent_info
                .set_label(&format!("UTF-8  |  {}", style.label()));

            // Reformat automatically if valid JSON
            let buf = imp.source_inspector.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
            if let Ok(formatted) = format_json_with_indent(&text, true, style) {
                buf.set_text(&formatted);
            }
        });

        // Mode toggle buttons in Inspector subheader
        let win_m_json = self.clone();
        imp.btn_mode_json.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_json.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "json");
                win_m_json.update_inspector_metrics();
            }
        });

        let win_m_vd = self.clone();
        imp.btn_mode_vardump.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_vd.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "php");
                win_m_vd.update_inspector_metrics();
            }
        });

        let win_m_raw = self.clone();
        imp.btn_mode_raw.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_raw.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "json");
                win_m_raw.update_inspector_metrics();
            }
        });

        let win_m_xml = self.clone();
        imp.btn_mode_xml.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_xml.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "xml");
                win_m_xml.update_inspector_metrics();
            }
        });

        // Inspector buffer changes -> live AST metrics & validation
        let win_ibuf = self.clone();
        imp.source_inspector.buffer().connect_changed(move |_| {
            win_ibuf.update_inspector_metrics();
            if let Some(ctrl) = win_ibuf.imp().fold_inspector.get() {
                ctrl.update_regions();
            }
        });

        // Diff buffers changes -> debounced live diff computation
        let win_dbuf_l = self.clone();
        imp.source_diff_left.buffer().connect_changed(move |_| {
            win_dbuf_l.schedule_diff_update();
            if let Some(ctrl) = win_dbuf_l.imp().fold_diff_left.get() {
                ctrl.update_regions();
            }
        });

        let win_dbuf_r = self.clone();
        imp.source_diff_right.buffer().connect_changed(move |_| {
            win_dbuf_r.schedule_diff_update();
            if let Some(ctrl) = win_dbuf_r.imp().fold_diff_right.get() {
                ctrl.update_regions();
            }
        });

        // Cursor position tracking: Inspector
        let lbl_ins_cur = imp.lbl_inspector_cursor.clone();
        imp.source_inspector
            .buffer()
            .connect_cursor_position_notify(move |buf| {
                let iter = buf.iter_at_offset(buf.cursor_position());
                lbl_ins_cur.set_label(&format!("Ln {}, Col {}", iter.line() + 1, iter.line_offset() + 1));
            });

        // Cursor position tracking: Diff Left & Right
        let lbl_diff_cur1 = imp.lbl_diff_cursor.clone();
        imp.source_diff_left
            .buffer()
            .connect_cursor_position_notify(move |buf| {
                let iter = buf.iter_at_offset(buf.cursor_position());
                lbl_diff_cur1.set_label(&format!("Ln {}, Col {}", iter.line() + 1, iter.line_offset() + 1));
            });

        let lbl_diff_cur2 = imp.lbl_diff_cursor.clone();
        imp.source_diff_right
            .buffer()
            .connect_cursor_position_notify(move |buf| {
                let iter = buf.iter_at_offset(buf.cursor_position());
                lbl_diff_cur2.set_label(&format!("Ln {}, Col {}", iter.line() + 1, iter.line_offset() + 1));
            });

        // Preferences signals
        let win_p_ln = self.clone();
        imp.pref_line_numbers.connect_active_notify(move |row| {
            let active = row.is_active();
            let imp = win_p_ln.imp();
            imp.source_diff_left.set_show_line_numbers(active);
            imp.source_diff_right.set_show_line_numbers(active);
            imp.source_inspector.set_show_line_numbers(active);
        });

        let win_p_map = self.clone();
        imp.pref_minimap.connect_active_notify(move |row| {
            win_p_map.imp().source_map.set_visible(row.is_active());
        });

        let win_p_hl = self.clone();
        imp.pref_highlight_line.connect_active_notify(move |row| {
            let active = row.is_active();
            let imp = win_p_hl.imp();
            imp.source_diff_left.set_highlight_current_line(active);
            imp.source_diff_right.set_highlight_current_line(active);
            imp.source_inspector.set_highlight_current_line(active);
        });

        let win_p_wrap = self.clone();
        imp.pref_word_wrap.connect_active_notify(move |row| {
            let mode = if row.is_active() {
                gtk4::WrapMode::WordChar
            } else {
                gtk4::WrapMode::None
            };
            let imp = win_p_wrap.imp();
            imp.source_diff_left.set_wrap_mode(mode);
            imp.source_diff_right.set_wrap_mode(mode);
            imp.source_inspector.set_wrap_mode(mode);
        });

        let win_p_pat = self.clone();
        imp.pref_patience.connect_active_notify(move |_| {
            win_p_pat.handle_compute_diff();
        });
    }

    fn setup_minimap(&self) {
        let imp = self.imp();
        let map = imp.source_map.get();
        let insp = imp.source_inspector.get();
        let map_weak = map.downgrade();
        let insp_weak = insp.downgrade();

        let scroll_to_pos = move |y: f64| {
            if let (Some(map), Some(insp)) = (map_weak.upgrade(), insp_weak.upgrade()) {
                let map_h = map.height() as f64;
                if map_h <= 0.0 {
                    return;
                }

                // Obtener o calcular la altura real del deslizador (slider)
                let slider_h = map
                    .first_child()
                    .map(|c| c.height() as f64)
                    .filter(|&h| h > 10.0)
                    .unwrap_or_else(|| {
                        if let Some(vadj) = insp.vadjustment() {
                            let total = vadj.upper() - vadj.lower();
                            if total > 0.0 {
                                ((vadj.page_size() / total) * map_h).clamp(24.0, map_h)
                            } else {
                                30.0
                            }
                        } else {
                            30.0
                        }
                    });

                // Centrar el deslizador en la posición exacta del cursor
                let available_h = (map_h - slider_h).max(1.0);
                let ratio = ((y - slider_h * 0.5) / available_h).clamp(0.0, 1.0);

                if let Some(vadj) = insp.vadjustment() {
                    let max_scroll = (vadj.upper() - vadj.page_size()).max(vadj.lower());
                    let target_val = vadj.lower() + ratio * (max_scroll - vadj.lower());
                    vadj.set_value(target_val);
                }

                let buf = insp.buffer();
                let total_lines = buf.line_count();
                if total_lines > 0 {
                    let target_line = ((total_lines - 1) as f64 * ratio).round() as i32;
                    if let Some(iter) = buf.iter_at_line(target_line) {
                        buf.place_cursor(&iter);
                    }
                }

                map.queue_draw();
            }
        };

        let drag = gtk4::GestureDrag::new();
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let on_begin = scroll_to_pos.clone();
        let insp_focus = insp.clone();
        drag.connect_drag_begin(move |gesture, _x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            insp_focus.grab_focus();
            on_begin(y);
        });

        let on_update = scroll_to_pos.clone();
        drag.connect_drag_update(move |gesture, _offset_x, offset_y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some((_start_x, start_y)) = gesture.start_point() {
                on_update(start_y + offset_y);
            }
        });

        map.add_controller(drag);

        let scroll_ctrl = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::VERTICAL,
        );
        scroll_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let insp_scroll = insp.clone();
        scroll_ctrl.connect_scroll(move |_ctrl, _dx, dy| {
            if let Some(vadj) = insp_scroll.vadjustment() {
                let step = vadj.step_increment().max(20.0);
                let max_val = (vadj.upper() - vadj.page_size()).max(vadj.lower());
                let new_val = (vadj.value() + dy * step * 3.0).clamp(vadj.lower(), max_val);
                vadj.set_value(new_val);
            }
            glib::Propagation::Stop
        });
        map.add_controller(scroll_ctrl);
    }

    fn setup_shortcuts(&self) {
        let controller = gtk4::ShortcutController::new();
        controller.set_scope(gtk4::ShortcutScope::Managed);

        // Ctrl+O -> win.open-file
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>o") {
            let action = gtk4::NamedAction::new("win.open-file");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+[ -> win.diff-prev
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>bracketleft") {
            let action = gtk4::NamedAction::new("win.diff-prev");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+] -> win.diff-next
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>bracketright") {
            let action = gtk4::NamedAction::new("win.diff-next");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }

        self.add_controller(controller);
    }

    fn setup_actions(&self) {
        // Open file dialog action
        let self_open = self.clone();
        let act_open = gio::SimpleAction::new("open-file", None);
        act_open.connect_activate(move |_, _| {
            self_open.handle_open_file(false);
        });
        self.add_action(&act_open);

        // Open specifically into left diff panel
        let self_open_l = self.clone();
        let act_open_l = gio::SimpleAction::new("open-left", None);
        act_open_l.connect_activate(move |_, _| {
            self_open_l.handle_open_file_target(FileTarget::DiffLeft);
        });
        self.add_action(&act_open_l);

        // Open specifically into right diff panel
        let self_open_r = self.clone();
        let act_open_r = gio::SimpleAction::new("open-right", None);
        act_open_r.connect_activate(move |_, _| {
            self_open_r.handle_open_file_target(FileTarget::DiffRight);
        });
        self.add_action(&act_open_r);

        // Paste into left diff panel
        let self_pst_l = self.clone();
        let act_pst_l = gio::SimpleAction::new("paste-left", None);
        act_pst_l.connect_activate(move |_, _| {
            let win_clone = self_pst_l.clone();
            self_pst_l.clipboard().read_text_async(
                gio::Cancellable::NONE,
                move |res| {
                    if let Ok(Some(text)) = res {
                        win_clone.imp().source_diff_left.buffer().set_text(&text);
                        win_clone.handle_compute_diff();
                    }
                },
            );
        });
        self.add_action(&act_pst_l);

        // Copy left diff panel
        let self_cpy_l = self.clone();
        let act_cpy_l = gio::SimpleAction::new("copy-left", None);
        act_cpy_l.connect_activate(move |_, _| {
            let buf = self_cpy_l.imp().source_diff_left.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
            if !text.is_empty() {
                self_cpy_l.clipboard().set_text(&text);
            }
        });
        self.add_action(&act_cpy_l);

        // Copy right diff panel
        let self_cpy_r = self.clone();
        let act_cpy_r = gio::SimpleAction::new("copy-right", None);
        act_cpy_r.connect_activate(move |_, _| {
            let buf = self_cpy_r.imp().source_diff_right.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
            if !text.is_empty() {
                self_cpy_r.clipboard().set_text(&text);
            }
        });
        self.add_action(&act_cpy_r);

        // Paste into right diff panel
        let self_pst_r = self.clone();
        let act_pst_r = gio::SimpleAction::new("paste-right", None);
        act_pst_r.connect_activate(move |_, _| {
            let win_clone = self_pst_r.clone();
            self_pst_r.clipboard().read_text_async(
                gio::Cancellable::NONE,
                move |res| {
                    if let Ok(Some(text)) = res {
                        win_clone.imp().source_diff_right.buffer().set_text(&text);
                        win_clone.handle_compute_diff();
                    }
                },
            );
        });
        self.add_action(&act_pst_r);

        // Diff navigation: previous
        let self_prev = self.clone();
        let act_prev = gio::SimpleAction::new("diff-prev", None);
        act_prev.connect_activate(move |_, _| {
            self_prev.jump_diff_chunk(false);
        });
        self.add_action(&act_prev);

        // Diff navigation: next
        let self_next = self.clone();
        let act_next = gio::SimpleAction::new("diff-next", None);
        act_next.connect_activate(move |_, _| {
            self_next.jump_diff_chunk(true);
        });
        self.add_action(&act_next);

        // Compute diff
        let self_diff = self.clone();
        let act_diff = gio::SimpleAction::new("compute-diff", None);
        act_diff.connect_activate(move |_, _| {
            self_diff.handle_compute_diff();
        });
        self.add_action(&act_diff);

        // Clear diff
        let self_clr_diff = self.clone();
        let act_clr_diff = gio::SimpleAction::new("clear-diff", None);
        act_clr_diff.connect_activate(move |_, _| {
            let imp = self_clr_diff.imp();
            imp.source_diff_left.buffer().set_text("");
            imp.source_diff_right.buffer().set_text("");
            self_clr_diff.clear_diff_highlights();
            imp.lbl_diff_count.set_label("0 diferencias encontradas");
            imp.lbl_diff_removals.set_label("0 supresiones (-)");
            imp.lbl_diff_additions.set_label("0 adiciones (+)");
            imp.lbl_footer_diff_badge.set_label("+0  -0");
            imp.hunk_positions.borrow_mut().clear();
        });
        self.add_action(&act_clr_diff);

        // Format JSON
        let self_fmt = self.clone();
        let act_fmt = gio::SimpleAction::new("format-json", None);
        act_fmt.connect_activate(move |_, _| {
            self_fmt.handle_json_format(true);
        });
        self.add_action(&act_fmt);

        // Convert var_dump
        let self_vd = self.clone();
        let act_vd = gio::SimpleAction::new("convert-vardump", None);
        act_vd.connect_activate(move |_, _| {
            self_vd.handle_vardump_convert();
        });
        self.add_action(&act_vd);

        // Minify JSON
        let self_min = self.clone();
        let act_min = gio::SimpleAction::new("minify-json", None);
        act_min.connect_activate(move |_, _| {
            self_min.handle_json_format(false);
        });
        self.add_action(&act_min);

        // Paste into Inspector
        let self_pst_ins = self.clone();
        let act_pst_ins = gio::SimpleAction::new("paste-inspector", None);
        act_pst_ins.connect_activate(move |_, _| {
            let win_clone = self_pst_ins.clone();
            self_pst_ins.clipboard().read_text_async(
                gio::Cancellable::NONE,
                move |res| {
                    if let Ok(Some(text)) = res {
                        let imp = win_clone.imp();
                        imp.source_inspector.buffer().set_text(&text);
                        win_clone.update_inspector_metrics();
                    }
                },
            );
        });
        self.add_action(&act_pst_ins);

        // Copy Inspector (with temporary button confirmation feedback)
        let self_cpy = self.clone();
        let act_cpy = gio::SimpleAction::new("copy-inspector", None);
        act_cpy.connect_activate(move |_, _| {
            let imp = self_cpy.imp();
            let buf = imp.source_inspector.buffer();
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();
            if !text.is_empty() {
                self_cpy.clipboard().set_text(&text);
                imp.lbl_inspector_status.set_label("Copiado al portapapeles");
                imp.btn_copy_inspector.set_tooltip_text(Some("Copiado"));
                let btn_clone = imp.btn_copy_inspector.clone();
                glib::timeout_add_local_once(Duration::from_millis(1800), move || {
                    btn_clone.set_tooltip_text(Some("Copiar al portapapeles"));
                });
            }
        });
        self.add_action(&act_cpy);

        // Clear Inspector
        let self_clr_ins = self.clone();
        let act_clr_ins = gio::SimpleAction::new("clear-inspector", None);
        act_clr_ins.connect_activate(move |_, _| {
            let imp = self_clr_ins.imp();
            imp.source_inspector.buffer().set_text("");
            imp.lbl_inspector_file.set_label("Sin archivo");
            imp.lbl_inspector_status.set_label("Editor limpiado");
        });
        self.add_action(&act_clr_ins);

        // Help Overlay
        let self_help = self.clone();
        let act_help = gio::SimpleAction::new("show-help-overlay", None);
        act_help.connect_activate(move |_, _| {
            self_help.show_help_dialog();
        });
        self.add_action(&act_help);
    }

    fn update_inspector_metrics(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();

        if text.trim().is_empty() {
            imp.lbl_syntax_valid.set_label("Editor vacío");
            imp.lbl_node_count.set_label("0 nodos");
            imp.lbl_payload_size.set_label("0 B");
            imp.lbl_tree_stats.set_label("Árbol: 0 niveles | 0 elementos");
            imp.lbl_inspector_status.set_label("Listo");
            imp.lbl_inspector_status.set_tooltip_text(None);
            imp.btn_format_json.set_tooltip_text(Some("Formatear (Ctrl+Shift+F)"));
            imp.btn_minify_json.set_tooltip_text(Some("Minificar"));
            imp.btn_vardump_to_json.set_tooltip_text(Some("Convertir a JSON"));
            return;
        }

        if imp.btn_mode_xml.is_active() {
            imp.btn_format_json.set_tooltip_text(Some("Formatear XML (Ctrl+Shift+F)"));
            imp.btn_minify_json.set_tooltip_text(Some("Minificar XML"));
            imp.btn_vardump_to_json.set_tooltip_text(Some("Convertir XML → JSON"));

            match analyze_xml(&text) {
                Ok(metrics) => {
                    if imp.lbl_inspector_status.label().as_str().starts_with("Error:") {
                        imp.lbl_inspector_status.set_label("Listo");
                        imp.lbl_inspector_status.set_tooltip_text(None);
                    }

                    imp.lbl_syntax_valid.set_label("✓ XML Válido");
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_rfc_badge.set_label("Válido W3C XML");
                    imp.lbl_rfc_badge.remove_css_class("badge-error");
                    imp.lbl_rfc_badge.add_css_class("badge-success");

                    imp.lbl_node_count
                        .set_label(&format!("{} elementos", metrics.total_elements));
                    imp.lbl_payload_size.set_label(&metrics.formatted_size());
                    imp.lbl_tree_stats.set_label(&format!(
                        "Árbol XML: {} niveles | {} atributos",
                        metrics.max_depth, metrics.total_attributes
                    ));
                }
                Err(err_msg) => {
                    imp.lbl_syntax_valid.set_label("✗ XML Inválido");
                    imp.lbl_syntax_valid.remove_css_class("badge-success");
                    imp.lbl_syntax_valid.add_css_class("badge-error");

                    imp.lbl_rfc_badge.set_label("Error sintáctico");
                    imp.lbl_rfc_badge.remove_css_class("badge-success");
                    imp.lbl_rfc_badge.add_css_class("badge-error");

                    imp.lbl_inspector_status.set_label(&err_msg);
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err_msg));
                }
            }
            return;
        }

        if imp.btn_mode_vardump.is_active() {
            imp.btn_format_json.set_tooltip_text(Some("Formatear (Ctrl+Shift+F)"));
            imp.btn_minify_json.set_tooltip_text(Some("Minificar"));
            imp.btn_vardump_to_json.set_tooltip_text(Some("Convertir var_dump → JSON"));

            if parse_vardump(&text).is_ok() {
                imp.lbl_syntax_valid.set_label("✓ var_dump Válido");
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_rfc_badge.set_label("AST PHP Listo");
                imp.lbl_rfc_badge.remove_css_class("badge-error");
                imp.lbl_rfc_badge.add_css_class("badge-success");

                let kb = text.len() as f64 / 1024.0;
                imp.lbl_payload_size.set_label(&format!("{:.1} KB", kb));
                imp.lbl_tree_stats.set_label("Estructura PHP var_dump detectada");
            } else {
                imp.lbl_syntax_valid.set_label("✗ var_dump Inválido");
                imp.lbl_syntax_valid.remove_css_class("badge-success");
                imp.lbl_syntax_valid.add_css_class("badge-error");

                imp.lbl_rfc_badge.set_label("Error sintáctico");
                imp.lbl_rfc_badge.remove_css_class("badge-success");
                imp.lbl_rfc_badge.add_css_class("badge-error");
            }
            return;
        }

        match analyze_json(&text) {
            Ok(metrics) => {
                if imp.lbl_inspector_status.label().as_str().starts_with("Error:") {
                    imp.lbl_inspector_status.set_label("Listo");
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }

                imp.btn_format_json.set_tooltip_text(Some("Formatear JSON (Ctrl+Shift+F)"));
                imp.btn_minify_json.set_tooltip_text(Some("Minificar JSON"));
                imp.btn_vardump_to_json.set_tooltip_text(Some("Convertir a JSON"));

                imp.lbl_syntax_valid.set_label("✓ Sintaxis Válida");
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_rfc_badge.set_label("Válido RFC 8259");
                imp.lbl_rfc_badge.remove_css_class("badge-error");
                imp.lbl_rfc_badge.add_css_class("badge-success");

                imp.lbl_node_count
                    .set_label(&format!("{} nodos", metrics.total_nodes));
                imp.lbl_payload_size.set_label(&metrics.formatted_size());
                imp.lbl_tree_stats.set_label(&format!(
                    "Árbol: {} niveles | {} claves",
                    metrics.max_depth, metrics.total_keys
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
                    imp.btn_mode_vardump.set_active(true);
                    return;
                }

                imp.lbl_syntax_valid.set_label("✗ Sintaxis Inválida");
                imp.lbl_syntax_valid.remove_css_class("badge-success");
                imp.lbl_syntax_valid.add_css_class("badge-error");

                imp.lbl_rfc_badge.set_label(&err_loc);
                imp.lbl_rfc_badge.remove_css_class("badge-success");
                imp.lbl_rfc_badge.add_css_class("badge-error");
            }
        }
    }

    fn handle_open_file(&self, force_left_diff: bool) {
        let target = if force_left_diff {
            FileTarget::DiffLeft
        } else {
            FileTarget::Active
        };
        self.handle_open_file_target(target);
    }

    fn handle_open_file_target(&self, target: FileTarget) {
        let file_dialog = gtk4::FileDialog::builder()
            .title("Abrir archivo")
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
                                        "Archivo cargado: {} líneas",
                                        count_lines(&text)
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    fn clear_diff_highlights(&self) {
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

    fn jump_diff_chunk(&self, next: bool) {
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

    fn schedule_diff_update(&self) {
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

    fn handle_compute_diff(&self) {
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
            imp.lbl_diff_count.set_label("Calculando...");
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

    fn apply_diff_summary(&self, summary: DiffSummary) {
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

        // 2. Inline word/token-level highlights
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

        let count_label = if summary.hunk_count == 1 {
            "1 diferencia encontrada".to_string()
        } else {
            format!("{} diferencias encontradas", summary.hunk_count)
        };
        imp.lbl_diff_count.set_label(&count_label);

        imp.lbl_diff_removals
            .set_label(&format!("{} supresiones (-)", summary.deletions));
        imp.lbl_diff_additions
            .set_label(&format!("{} adiciones (+)", summary.insertions));
        imp.lbl_footer_diff_badge
            .set_label(&format!("+{}  -{}", summary.insertions, summary.deletions));

        imp.hunk_positions.replace(summary.hunk_positions);
        imp.current_hunk_idx.set(0);
    }

    fn handle_json_format(&self, pretty: bool) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let lines = count_lines(&text);
        let indent = self.current_indent_style();

        if text.trim().is_empty() {
            imp.lbl_inspector_status.set_label("El editor está vacío");
            return;
        }

        let is_xml = imp.btn_mode_xml.is_active() || text.trim_start().starts_with('<');

        let action_name = if is_xml {
            if pretty { "Formateando XML" } else { "Minificando XML" }
        } else if pretty {
            "Formateando JSON"
        } else {
            "Minificando JSON"
        };

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status
                .set_label(&format!("{} ({} líneas)...", action_name, lines));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = if is_xml {
                    match format_xml_with_indent(&text, pretty, indent) {
                        Ok(formatted) => WorkerResult::InspectorFormatted {
                            text: formatted,
                            message: if pretty {
                                "XML formateado correctamente".to_string()
                            } else {
                                "XML minificado correctamente".to_string()
                            },
                        },
                        Err(e) => WorkerResult::Error(e),
                    }
                } else {
                    match format_json_with_indent(&text, pretty, indent) {
                        Ok(formatted) => WorkerResult::InspectorFormatted {
                            text: formatted,
                            message: if pretty {
                                "JSON formateado correctamente".to_string()
                            } else {
                                "JSON minificado correctamente".to_string()
                            },
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
                        "XML formateado"
                    } else {
                        "XML minificado"
                    });
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status.set_label(&format!("Error: {}", err));
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
                        "JSON formateado"
                    } else {
                        "JSON minificado"
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

    fn handle_vardump_convert(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), true)
            .to_string();
        let lines = count_lines(&text);

        if text.trim().is_empty() {
            imp.lbl_inspector_status.set_label("El editor está vacío");
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
                        .set_label("XML transformado exitosamente a JSON");
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label("Error: No es un XML válido para convertir a JSON");
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
            return;
        }

        // Si ya es un JSON válido, informar amablemente al usuario
        if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
            imp.lbl_inspector_status
                .set_label("El contenido ya es JSON válido (no requiere conversión)");
            imp.lbl_inspector_status.set_tooltip_text(None);
            return;
        }

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status
                .set_label(&format!("Parseando var_dump ({} líneas)...", lines));
            imp.lbl_inspector_status.set_tooltip_text(None);
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = match vardump_to_json_string(&text, true) {
                    Ok(json_str) => WorkerResult::InspectorFormatted {
                        text: json_str,
                        message: "var_dump transformado exitosamente a JSON".to_string(),
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
                        .set_label("var_dump convertido a JSON");
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label("Error: No es una estructura PHP var_dump válida");
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err));
                }
            }
        }
    }

    fn handle_worker_result(&self, result: WorkerResult) {
        let imp = self.imp();
        match result {
            WorkerResult::InspectorFormatted { text, message } => {
                let buf = imp.source_inspector.buffer();
                buf.set_text(&text);
                Self::set_buffer_language(&buf, "json");
                imp.btn_mode_json.set_active(true);
                imp.lbl_inspector_status.set_label(&message);
                imp.lbl_inspector_status.set_tooltip_text(None);
            }
            WorkerResult::DiffCalculated(summary) => {
                self.apply_diff_summary(summary);
            }
            WorkerResult::Error(err) => {
                imp.lbl_inspector_status
                    .set_label("Error: No es una estructura PHP var_dump válida");
                imp.lbl_inspector_status.set_tooltip_text(Some(&err));
            }
        }
    }

    fn show_help_dialog(&self) {
        let dialog = adw::MessageDialog::builder()
            .heading("Atajos del Teclado y Ayuda")
            .body(
                "• Ctrl+O: Abrir archivo en el panel activo\n\
                 • Ctrl+[ / Ctrl+]: Navegar a la diferencia anterior o siguiente\n\
                 • Formatear JSON: Sangría de 2/4 espacios o tabuladores y validación RFC 8259\n\
                 • var_dump → JSON: Parsea volcados recursivos de PHP a JSON estructurado\n\
                 • Comparador: Resaltado de línea y token (inline diff) con opción de ignorar espacios\n\
                 • Hilos en 2do plano: Cargas mayores a 1000 líneas se procesan asíncronamente",
            )
            .transient_for(self)
            .modal(true)
            .build();
        dialog.add_response("ok", "Entendido");
        dialog.set_default_response(Some("ok"));
        dialog.present();
    }
}
