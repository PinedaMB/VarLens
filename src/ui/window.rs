use crate::core::diff::{compute_diff_ext, DiffSummary};
use crate::core::json::{
    analyze_json, count_lines, format_json_with_indent, sort_json_keys, IndentStyle,
};
use crate::core::vardump::{is_printr_raw, parse_vardump, vardump_to_json_string};
use crate::core::xml::{analyze_xml, format_xml_with_indent, xml_to_json_string};
use crate::i18n::{self, Language};
use crate::tr;
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
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const ASYNC_LINE_THRESHOLD: usize = 1000;

enum WorkerResult {
    InspectorFormatted {
        text: String,
        message: String,
        is_xml: bool,
    },
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
        #[template_child]
        pub page_diff: TemplateChild<adw::ViewStackPage>,
        #[template_child]
        pub page_inspector: TemplateChild<adw::ViewStackPage>,
        #[template_child]
        pub page_preferences: TemplateChild<adw::ViewStackPage>,

        // Top Header widgets
        #[template_child]
        pub btn_open: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_save: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_load_demo: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_theme_toggle: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_primary_menu: TemplateChild<gtk4::MenuButton>,

        // Diff page widgets
        #[template_child]
        pub diff_paned: TemplateChild<gtk4::Paned>,
        #[template_child]
        pub btn_diff_prev: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_diff_next: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_run_diff: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_swap_diff: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_normalize_diff: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_copy_patch: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_clear_diff: TemplateChild<gtk4::Button>,
        #[template_child]
        pub source_diff_left: TemplateChild<crate::ui::diff_editor::VarLensDiffView>,
        #[template_child]
        pub source_diff_right: TemplateChild<crate::ui::diff_editor::VarLensDiffView>,
        #[template_child]
        pub lbl_title_original: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_title_modified: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_left_file: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_right_file: TemplateChild<gtk4::Label>,
        #[template_child]
        pub btn_copy_left: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_paste_left: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_load_left: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_copy_right: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_paste_right: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_load_right: TemplateChild<gtk4::Button>,
        #[template_child]
        pub lbl_diff_count: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_similarity: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_detect_mode: TemplateChild<gtk4::Label>,
        #[template_child]
        pub sw_ignore_whitespace: TemplateChild<gtk4::Switch>,
        #[template_child]
        pub lbl_ignore_whitespace: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_removals: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_additions: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_sync: TemplateChild<gtk4::Label>,
        #[template_child]
        pub lbl_diff_encoding: TemplateChild<gtk4::Label>,
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
        pub entry_search_inspector: TemplateChild<gtk4::SearchEntry>,
        #[template_child]
        pub dd_indent: TemplateChild<gtk4::DropDown>,
        #[template_child]
        pub btn_paste_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_copy_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_clear_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_format_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_vardump_to_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_minify_json: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_sort_keys: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_mode_json: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_vardump: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_raw: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_xml: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_insp_view_code: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_insp_view_tree: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub stack_inspector_view: TemplateChild<gtk4::Stack>,
        #[template_child]
        pub box_tree_container: TemplateChild<gtk4::Box>,
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
        pub grp_pref_general: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub pref_language: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub grp_pref_editor: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub pref_line_numbers: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_minimap: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_highlight_line: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub pref_word_wrap: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub grp_pref_diff: TemplateChild<adw::PreferencesGroup>,
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
        pub last_unified_diff: RefCell<String>,
        pub current_hunk_idx: Cell<usize>,
        pub diff_debounce_source: RefCell<Option<glib::SourceId>>,
        pub fold_inspector: OnceCell<FoldingController>,
        pub fold_diff_left: OnceCell<FoldingController>,
        pub fold_diff_right: OnceCell<FoldingController>,
        pub json_tree: OnceCell<crate::ui::json_tree::JsonTreeView>,
        pub search_context: OnceCell<sourceview5::SearchContext>,
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
        let style_manager = adw::StyleManager::default();
        let is_dark_init = style_manager.is_dark();
        let (del_bg, ins_bg) = if is_dark_init {
            (
                gdk::RGBA::new(0.92, 0.15, 0.18, 0.20),
                gdk::RGBA::new(0.0, 0.62, 0.35, 0.20),
            )
        } else {
            (
                gdk::RGBA::new(0.96, 0.32, 0.35, 0.14),
                gdk::RGBA::new(0.10, 0.68, 0.35, 0.14),
            )
        };

        let tag_del = gtk4::TextTag::builder()
            .name("diff-deleted")
            .paragraph_background_rgba(&del_bg)
            .build();

        let tag_ins = gtk4::TextTag::builder()
            .name("diff-inserted")
            .paragraph_background_rgba(&ins_bg)
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

        imp.tag_deleted.set(tag_del.clone()).ok();
        imp.tag_inserted.set(tag_ins.clone()).ok();
        imp.tag_span_deleted.set(tag_span_del).ok();
        imp.tag_span_inserted.set(tag_span_ins).ok();

        // 2. Configure Style Schemes & Dark/Light Mode Adaptivity
        Self::apply_schemes(
            &left_buf,
            &right_buf,
            &inspector_buf,
            is_dark_init,
        );
        imp.btn_theme_toggle.set_icon_name(if is_dark_init {
            "weather-clear-symbolic"
        } else {
            "weather-clear-night-symbolic"
        });

        let lb_clone = left_buf.clone();
        let rb_clone = right_buf.clone();
        let ib_clone = inspector_buf.clone();
        let tag_del_clone = tag_del;
        let tag_ins_clone = tag_ins;
        let win_theme_weak = self.downgrade();
        style_manager.connect_dark_notify(move |sm| {
            let is_dark = sm.is_dark();
            Self::apply_schemes(&lb_clone, &rb_clone, &ib_clone, is_dark);
            if is_dark {
                tag_del_clone.set_paragraph_background_rgba(Some(&gdk::RGBA::new(0.92, 0.15, 0.18, 0.20)));
                tag_ins_clone.set_paragraph_background_rgba(Some(&gdk::RGBA::new(0.0, 0.62, 0.35, 0.20)));
            } else {
                tag_del_clone.set_paragraph_background_rgba(Some(&gdk::RGBA::new(0.96, 0.32, 0.35, 0.14)));
                tag_ins_clone.set_paragraph_background_rgba(Some(&gdk::RGBA::new(0.10, 0.68, 0.35, 0.14)));
            }
            if let Some(win) = win_theme_weak.upgrade() {
                let imp = win.imp();
                imp.btn_theme_toggle.set_icon_name(if is_dark {
                    "weather-clear-symbolic"
                } else {
                    "weather-clear-night-symbolic"
                });
                imp.source_diff_left.queue_draw();
                imp.source_diff_right.queue_draw();
                imp.box_tree_container.queue_draw();
                if let Some(tree) = imp.json_tree.get() {
                    tree.refresh_theme(is_dark);
                }
                if let Some(fi) = imp.fold_inspector.get() {
                    fi.queue_draw();
                }
                if let Some(fl) = imp.fold_diff_left.get() {
                    fl.queue_draw();
                }
                if let Some(fr) = imp.fold_diff_right.get() {
                    fr.queue_draw();
                }
            }
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

        // 8. Setup Interactive JSON Tree View
        let json_tree = crate::ui::json_tree::JsonTreeView::new();
        imp.box_tree_container.append(json_tree.widget());
        imp.json_tree.set(json_tree).ok();

        let win_tree = self.clone();
        imp.btn_insp_view_tree.connect_toggled(move |btn| {
            if btn.is_active() {
                win_tree.switch_to_tree_view();
            }
        });

        let win_code = self.clone();
        imp.btn_insp_view_code.connect_toggled(move |btn| {
            if btn.is_active() {
                win_code.imp().stack_inspector_view.set_visible_child_name("page_code");
            }
        });

        // 9. Register Window Actions & Shortcuts
        self.setup_actions();
        self.setup_shortcuts();
        self.setup_code_search();
        self.setup_drag_and_drop();

        // 10. Start with clean editors and initialize metrics & language texts
        self.update_ui_texts();
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
                Self::set_buffer_language(&imp.source_inspector.buffer(), "php");
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
            if let Some(ctrl) = imp.fold_diff_left.get() {
                ctrl.set_line_numbers_visible(active);
            }
            if let Some(ctrl) = imp.fold_diff_right.get() {
                ctrl.set_line_numbers_visible(active);
            }
            if let Some(ctrl) = imp.fold_inspector.get() {
                ctrl.set_line_numbers_visible(active);
            }
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

        // Language preference change signal
        let win_p_lang = self.clone();
        imp.pref_language.set_selected(i18n::current_language().as_u8() as u32);
        imp.pref_language.connect_selected_notify(move |row| {
            let sel = row.selected();
            let new_lang = Language::from_u8(sel as u8);
            if new_lang != i18n::current_language() {
                i18n::set_language(new_lang);
                win_p_lang.update_ui_texts();
            }
        });
    }

    pub fn update_ui_texts(&self) {
        let imp = self.imp();

        // 1. Stack page titles
        imp.page_diff.set_title(Some(tr!("Comparador", "Diff Viewer")));
        imp.page_inspector.set_title(Some(tr!("Inspector", "Inspector")));
        imp.page_preferences.set_title(Some(tr!("Preferencias", "Preferences")));

        // 2. Headerbar & Primary Menu
        imp.btn_open.set_label(tr!("Abrir", "Open"));
        imp.btn_open.set_tooltip_text(Some(tr!("Abrir archivo (Ctrl+O)", "Open file (Ctrl+O)")));
        imp.btn_save.set_tooltip_text(Some(tr!("Guardar archivo (Ctrl+S)", "Save file (Ctrl+S)")));
        imp.btn_load_demo.set_label(tr!("Demo", "Demo"));
        imp.btn_load_demo.set_tooltip_text(Some(tr!(
            "Cargar datos de demostración en la vista actual",
            "Load demo payload into active view"
        )));
        imp.btn_theme_toggle.set_tooltip_text(Some(tr!(
            "Alternar tema Claro / Oscuro",
            "Toggle Light / Dark theme"
        )));
        imp.btn_primary_menu.set_tooltip_text(Some(tr!("Menú principal", "Main menu")));
        imp.btn_load_demo.set_tooltip_text(Some(tr!(
            "Cargar ejemplo interactivo (Demo)",
            "Load interactive demo example"
        )));
        imp.btn_theme_toggle.set_tooltip_text(Some(tr!(
            "Alternar tema Claro / Oscuro",
            "Toggle Light / Dark theme"
        )));

        let menu = gio::Menu::new();
        let sec1 = gio::Menu::new();
        sec1.append(Some(tr!("Apoyar proyecto ❤️", "Support project ❤️")), Some("app.sponsor"));
        menu.append_section(None, &sec1);
        let sec2 = gio::Menu::new();
        sec2.append(
            Some(tr!("Atajos del teclado y detalles", "Keyboard shortcuts & details")),
            Some("win.show-help-overlay"),
        );
        sec2.append(Some(tr!("Acerca de VarLens", "About VarLens")), Some("app.about"));
        menu.append_section(None, &sec2);
        imp.btn_primary_menu.set_menu_model(Some(&menu));

        // 3. Diff controls
        imp.btn_diff_prev.set_label(tr!("Anterior", "Previous"));
        imp.btn_diff_prev.set_tooltip_text(Some(tr!("Diferencia anterior (Ctrl+[)", "Previous difference (Ctrl+[)")));
        imp.btn_diff_next.set_label(tr!("Siguiente", "Next"));
        imp.btn_diff_next.set_tooltip_text(Some(tr!("Siguiente diferencia (Ctrl+])", "Next difference (Ctrl+])")));
        imp.btn_run_diff.set_label(tr!("Comparar", "Compare"));
        imp.btn_run_diff.set_tooltip_text(Some(tr!("Recalcular diferencias", "Recalculate differences")));
        imp.btn_swap_diff.set_tooltip_text(Some(tr!(
            "Intercambiar paneles Izquierdo ↔ Derecho",
            "Swap Left ↔ Right panels"
        )));
        imp.btn_normalize_diff.set_tooltip_text(Some(tr!(
            "Formatear y normalizar ambos paneles antes de comparar",
            "Format and normalize both panels before comparing"
        )));
        imp.btn_copy_patch.set_tooltip_text(Some(tr!(
            "Copiar Unified Diff (.patch) al portapapeles",
            "Copy Unified Diff (.patch) to clipboard"
        )));
        imp.btn_clear_diff.set_label(tr!("Limpiar", "Clear"));
        imp.lbl_ignore_whitespace.set_label(tr!("Ignorar espacios", "Ignore whitespace"));
        imp.lbl_diff_sync.set_label(tr!("↻ Diferencias sincronizadas", "↻ Differences synchronized"));
        imp.lbl_diff_encoding.set_label(tr!("UTF-8  •  JSON  •  Espacios: 2", "UTF-8  •  JSON  •  Spaces: 2"));

        imp.lbl_title_original.set_label(tr!("Original (Base)", "Original (Base)"));
        imp.lbl_title_modified.set_label(tr!("Modificado (Nuevo)", "Modified (New)"));
        if imp.lbl_left_file.label() == "Sin archivo" || imp.lbl_left_file.label() == "No file" {
            imp.lbl_left_file.set_label(tr!("Sin archivo", "No file"));
        }
        if imp.lbl_right_file.label() == "Sin archivo" || imp.lbl_right_file.label() == "No file" {
            imp.lbl_right_file.set_label(tr!("Sin archivo", "No file"));
        }
        if imp.lbl_inspector_file.label() == "Sin archivo" || imp.lbl_inspector_file.label() == "No file" {
            imp.lbl_inspector_file.set_label(tr!("Sin archivo", "No file"));
        }
        imp.btn_copy_left.set_tooltip_text(Some(tr!("Copiar código original", "Copy original code")));
        imp.btn_paste_left.set_tooltip_text(Some(tr!("Pegar del portapapeles", "Paste from clipboard")));
        imp.btn_load_left.set_tooltip_text(Some(tr!("Cargar archivo base", "Load base file")));
        imp.btn_copy_right.set_tooltip_text(Some(tr!("Copiar código nuevo", "Copy modified code")));
        imp.btn_paste_right.set_tooltip_text(Some(tr!("Pegar del portapapeles", "Paste from clipboard")));
        imp.btn_load_right.set_tooltip_text(Some(tr!("Cargar archivo modificado", "Load modified file")));
        imp.lbl_detect_mode.set_label(tr!("Detectar: Auto (JSON, XML, var_dump)", "Detect: Auto (JSON, XML, var_dump)"));

        // 4. Inspector controls
        imp.btn_paste_inspector.set_tooltip_text(Some(tr!("Pegar", "Paste")));
        imp.btn_copy_inspector.set_tooltip_text(Some(tr!("Copiar", "Copy")));
        imp.btn_clear_inspector.set_tooltip_text(Some(tr!("Limpiar contenido", "Clear content")));
        imp.btn_sort_keys.set_tooltip_text(Some(tr!(
            "Ordenar claves JSON alfabéticamente (A→Z)",
            "Sort JSON keys alphabetically (A→Z)"
        )));
        imp.entry_search_inspector.set_placeholder_text(Some(tr!(
            "Buscar en código (Ctrl+F)...",
            "Search in code (Ctrl+F)..."
        )));
        imp.btn_insp_view_code.set_tooltip_text(Some(tr!("Vista de Código / Editor", "Code / Editor View")));
        imp.btn_insp_view_tree.set_tooltip_text(Some(tr!("Vista de Árbol Interactivo (Explorar nodos)", "Interactive Tree View (Explore nodes)")));

        // Indent dropdown
        let current_indent = imp.dd_indent.selected();
        let indent_model = gtk4::StringList::new(&[
            tr!("Indentación: 2 Espacios", "Indentation: 2 Spaces"),
            tr!("Indentación: 4 Espacios", "Indentation: 4 Spaces"),
            tr!("Indentación: Tabuladores", "Indentation: Tabs"),
        ]);
        imp.dd_indent.set_model(Some(&indent_model));
        imp.dd_indent.set_selected(current_indent);
        imp.lbl_indent_info.set_label(&format!("UTF-8  |  {}", self.current_indent_style().label()));

        // 5. Preferences
        imp.grp_pref_general.set_title(tr!("General / Idioma", "General / Language"));
        imp.grp_pref_general.set_description(Some(tr!("Ajustes globales de la interfaz", "Global interface settings")));
        imp.pref_language.set_title(tr!("Idioma de la interfaz", "Interface language"));
        imp.pref_language.set_subtitle(tr!("Selecciona el idioma (Español / English)", "Select language (Español / English)"));

        imp.grp_pref_editor.set_title(tr!("Editor de Código", "Code Editor"));
        imp.grp_pref_editor.set_description(Some(tr!("Personaliza la visualización de los editores GtkSourceView", "Customize GtkSourceView editors display")));
        imp.pref_line_numbers.set_title(tr!("Mostrar números de línea", "Show line numbers"));
        imp.pref_minimap.set_title(tr!("Mostrar minimapa en el Inspector", "Show minimap in Inspector"));
        imp.pref_highlight_line.set_title(tr!("Resaltar línea actual", "Highlight current line"));
        imp.pref_word_wrap.set_title(tr!("Ajuste automático de línea (Word Wrap)", "Automatic word wrap"));

        imp.grp_pref_diff.set_title(tr!("Motor de Diferencias (Diff Engine)", "Diff Engine"));
        imp.grp_pref_diff.set_description(Some(tr!("Ajustes del algoritmo de comparación del crate similar", "Comparison algorithm settings from similar crate")));
        imp.pref_patience.set_title(tr!("Algoritmo Patience", "Patience Algorithm"));
        imp.pref_patience.set_subtitle(tr!(
            "Produce diffs más semánticos para estructuras JSON y código fuente",
            "Produces more semantic diffs for JSON structures and source code"
        ));

        // 6. Tree View
        if let Some(tree) = imp.json_tree.get() {
            tree.update_language();
        }

        // 7. Refresh metrics and diff displays
        self.update_inspector_metrics();
        self.handle_compute_diff();
    }

    fn setup_minimap(&self) {
        let imp = self.imp();
        let map = imp.source_map.get();
        let insp = imp.source_inspector.get();

        // Prevent the minimap TextView from taking focus or running GtkSourceMap's
        // built-in GestureClick (which claims the sequence on press and kills GestureDrag).
        map.set_can_focus(false);
        map.set_editable(false);
        let controllers = map.observe_controllers();
        for i in 0..controllers.n_items() {
            if let Some(ctrl) = controllers.item(i).and_downcast::<gtk4::EventController>() {
                if ctrl.is::<gtk4::Gesture>() || ctrl.is::<gtk4::EventControllerScroll>() {
                    ctrl.set_propagation_phase(gtk4::PropagationPhase::None);
                }
            }
        }

        let map_weak = map.downgrade();
        let insp_weak = insp.downgrade();
        let grab_offset: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));

        let compute_metrics = move || -> Option<(
            sourceview5::Map,
            sourceview5::View,
            gtk4::Adjustment,
            f64,
            f64,
            f64,
            f64,
        )> {
            let map = map_weak.upgrade()?;
            let insp = insp_weak.upgrade()?;
            let vadj = insp.vadjustment()?;

            let map_h = map.height() as f64;
            if map_h <= 0.0 {
                return None;
            }

            let lower = vadj.lower();
            let upper = vadj.upper();
            let page_size = vadj.page_size();
            let max_scroll = (upper - page_size).max(lower);
            if max_scroll <= lower {
                return None;
            }

            let map_buf = map.buffer();
            let end_rect = map.iter_location(&map_buf.end_iter());
            let map_content_h = ((end_rect.y() + end_rect.height()) as f64).max(1.0);
            let track_h = map_h.min(map_content_h).max(1.0);

            let slider_h = ((page_size / (upper - lower).max(1.0)) * track_h).clamp(16.0, track_h);
            let available_h = (track_h - slider_h).max(1.0);

            Some((map, insp, vadj, lower, max_scroll, slider_h, available_h))
        };

        let compute_for_begin = compute_metrics.clone();
        let compute_for_update = compute_metrics;

        let apply_scroll = |map: &sourceview5::Map,
                            vadj: &gtk4::Adjustment,
                            lower: f64,
                            max_scroll: f64,
                            available_h: f64,
                            y: f64,
                            offset_in_slider: f64| {
            let target_slider_y = y - offset_in_slider;
            let ratio = (target_slider_y / available_h).clamp(0.0, 1.0);
            let target_val = lower + ratio * (max_scroll - lower);
            vadj.set_value(target_val);

            if let Some(map_vadj) = map.vadjustment() {
                let map_lower = map_vadj.lower();
                let map_max = (map_vadj.upper() - map_vadj.page_size()).max(map_lower);
                if map_max > map_lower {
                    map_vadj.set_value(map_lower + ratio * (map_max - map_lower));
                }
            }
            map.queue_resize();
            map.queue_draw();
        };

        let drag = gtk4::GestureDrag::new();
        drag.set_button(gtk4::gdk::BUTTON_PRIMARY);
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let grab_begin = grab_offset.clone();
        drag.connect_drag_begin(move |gesture, _x, start_y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some((map, _insp, vadj, lower, max_scroll, slider_h, available_h)) =
                compute_for_begin()
            {
                let mut child_opt = map.first_child();
                while let Some(child) = child_opt {
                    if child.css_name() == "slider" {
                        child.add_css_class("dragging");
                    }
                    child_opt = child.next_sibling();
                }

                let cur_ratio = ((vadj.value() - lower) / (max_scroll - lower)).clamp(0.0, 1.0);
                let cur_slider_y = cur_ratio * available_h;

                if start_y >= cur_slider_y && start_y <= cur_slider_y + slider_h {
                    // Clicked inside the slider: preserve relative grab offset
                    grab_begin.set(start_y - cur_slider_y);
                } else {
                    // Clicked outside the slider: center slider at click position and jump
                    let half = slider_h * 0.5;
                    grab_begin.set(half);
                    apply_scroll(&map, &vadj, lower, max_scroll, available_h, start_y, half);
                }
            }
        });

        let grab_update = grab_offset;
        drag.connect_drag_update(move |gesture, _offset_x, offset_y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some((_start_x, start_y)) = gesture.start_point() {
                if let Some((map, _insp, vadj, lower, max_scroll, _slider_h, available_h)) =
                    compute_for_update()
                {
                    apply_scroll(
                        &map,
                        &vadj,
                        lower,
                        max_scroll,
                        available_h,
                        start_y + offset_y,
                        grab_update.get(),
                    );
                }
            }
        });

        let map_end_weak = map.downgrade();
        drag.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
            if let Some(map) = map_end_weak.upgrade() {
                let mut child_opt = map.first_child();
                while let Some(child) = child_opt {
                    if child.css_name() == "slider" {
                        child.remove_css_class("dragging");
                    }
                    child_opt = child.next_sibling();
                }
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
                let step = vadj.step_increment().max(24.0);
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
        // Ctrl+S -> win.save-file
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>s") {
            let action = gtk4::NamedAction::new("win.save-file");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+F -> win.focus-search
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>f") {
            let action = gtk4::NamedAction::new("win.focus-search");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Shift+F -> win.format-json
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>f") {
            let action = gtk4::NamedAction::new("win.format-json");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Shift+M -> win.minify-json
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>m") {
            let action = gtk4::NamedAction::new("win.minify-json");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Shift+S -> win.sort-json-keys
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>s") {
            let action = gtk4::NamedAction::new("win.sort-json-keys");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Shift+J -> win.convert-vardump
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>j") {
            let action = gtk4::NamedAction::new("win.convert-vardump");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Return -> win.compute-diff
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>Return") {
            let action = gtk4::NamedAction::new("win.compute-diff");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+Shift+C / Ctrl+Shift+D -> win.clear-diff
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>c") {
            let action = gtk4::NamedAction::new("win.clear-diff");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control><Shift>d") {
            let action = gtk4::NamedAction::new("win.clear-diff");
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
        // F1 -> win.show-help-overlay
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("F1") {
            let action = gtk4::NamedAction::new("win.show-help-overlay");
            controller.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        }
        // Ctrl+? -> win.show-help-overlay
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string("<Control>question") {
            let action = gtk4::NamedAction::new("win.show-help-overlay");
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

        // Save file dialog action
        let self_save = self.clone();
        let act_save = gio::SimpleAction::new("save-file", None);
        act_save.connect_activate(move |_, _| {
            self_save.handle_save_file();
        });
        self.add_action(&act_save);

        // Toggle Light / Dark Theme action
        let self_theme = self.clone();
        let act_theme = gio::SimpleAction::new("toggle-theme", None);
        act_theme.connect_activate(move |_, _| {
            let sm = adw::StyleManager::default();
            if sm.is_dark() {
                sm.set_color_scheme(adw::ColorScheme::ForceLight);
                self_theme.imp().btn_theme_toggle.set_icon_name("weather-clear-night-symbolic");
            } else {
                sm.set_color_scheme(adw::ColorScheme::ForceDark);
                self_theme.imp().btn_theme_toggle.set_icon_name("weather-clear-symbolic");
            }
        });
        self.add_action(&act_theme);

        // Load interactive Demo payload action
        let self_demo = self.clone();
        let act_demo = gio::SimpleAction::new("load-demo", None);
        act_demo.connect_activate(move |_, _| {
            self_demo.handle_load_demo();
        });
        self.add_action(&act_demo);

        // Focus search bar action (Ctrl+F)
        let self_fsearch = self.clone();
        let act_fsearch = gio::SimpleAction::new("focus-search", None);
        act_fsearch.connect_activate(move |_, _| {
            let imp = self_fsearch.imp();
            let active = imp.main_stack.visible_child_name().unwrap_or_default();
            if active != "inspector_page" {
                imp.main_stack.set_visible_child_name("inspector_page");
            }
            imp.entry_search_inspector.grab_focus();
        });
        self.add_action(&act_fsearch);

        // Swap diff panels (Left <-> Right)
        let self_swap = self.clone();
        let act_swap = gio::SimpleAction::new("swap-diff", None);
        act_swap.connect_activate(move |_, _| {
            let imp = self_swap.imp();
            let left_buf = imp.source_diff_left.buffer();
            let right_buf = imp.source_diff_right.buffer();
            let left_text = left_buf
                .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
                .to_string();
            let right_text = right_buf
                .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
                .to_string();
            let left_file = imp.lbl_left_file.label().to_string();
            let right_file = imp.lbl_right_file.label().to_string();

            left_buf.set_text(&right_text);
            right_buf.set_text(&left_text);
            imp.lbl_left_file.set_label(&right_file);
            imp.lbl_right_file.set_label(&left_file);
            self_swap.handle_compute_diff();
        });
        self.add_action(&act_swap);

        // Normalize & format both diff panels
        let self_norm = self.clone();
        let act_norm = gio::SimpleAction::new("normalize-diff", None);
        act_norm.connect_activate(move |_, _| {
            self_norm.handle_normalize_diff();
        });
        self.add_action(&act_norm);

        // Copy Unified Diff (.patch)
        let self_patch = self.clone();
        let act_patch = gio::SimpleAction::new("copy-unified-diff", None);
        act_patch.connect_activate(move |_, _| {
            let imp = self_patch.imp();
            let patch = imp.last_unified_diff.borrow().clone();
            if !patch.is_empty() {
                self_patch.clipboard().set_text(&patch);
                imp.lbl_diff_sync.set_label(tr!(
                    "✓ Unified Diff (.patch) copiado al portapapeles",
                    "✓ Unified Diff (.patch) copied to clipboard"
                ));
                let lbl = imp.lbl_diff_sync.clone();
                glib::timeout_add_local_once(Duration::from_millis(2000), move || {
                    lbl.set_label(tr!(
                        "↻ Diferencias sincronizadas",
                        "↻ Differences synchronized"
                    ));
                });
            }
        });
        self.add_action(&act_patch);

        // Sort JSON keys alphabetically (A->Z)
        let self_sort = self.clone();
        let act_sort = gio::SimpleAction::new("sort-json-keys", None);
        act_sort.connect_activate(move |_, _| {
            self_sort.handle_sort_json_keys();
        });
        self.add_action(&act_sort);

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
            imp.lbl_left_file.set_label(tr!("Sin archivo", "No file"));
            imp.lbl_right_file.set_label(tr!("Sin archivo", "No file"));
            self_clr_diff.clear_diff_highlights();
            imp.lbl_diff_count.set_label(tr!("Sin diferencias", "No differences"));
            imp.lbl_diff_similarity.set_label(&format!("100% {}", tr!("similitud", "similarity")));
            imp.lbl_diff_removals.set_label(&format!("0 {}", tr!("supresiones (-)", "deletions (-)")));
            imp.lbl_diff_additions.set_label(&format!("0 {}", tr!("adiciones (+)", "additions (+)")));
            imp.lbl_footer_diff_badge.set_label("+0  -0");
            imp.hunk_positions.borrow_mut().clear();
            imp.last_unified_diff.borrow_mut().clear();
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
                imp.lbl_inspector_status.set_label(tr!("Copiado al portapapeles", "Copied to clipboard"));
                imp.btn_copy_inspector.set_tooltip_text(Some(tr!("Copiado", "Copied")));
                let btn_clone = imp.btn_copy_inspector.clone();
                glib::timeout_add_local_once(Duration::from_millis(1800), move || {
                    btn_clone.set_tooltip_text(Some(tr!("Copiar", "Copy")));
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
            imp.lbl_inspector_file.set_label(tr!("Sin archivo", "No file"));
            imp.lbl_inspector_status.set_label(tr!("Editor limpiado", "Editor cleared"));
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
            imp.lbl_syntax_valid.set_label(tr!("Listo", "Ready"));
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
            return;
        }

        if imp.btn_mode_xml.is_active() {
            imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear XML (Ctrl+Shift+F)", "Format XML (Ctrl+Shift+F)")));
            imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar XML", "Minify XML")));
            imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir XML → JSON", "Convert XML → JSON")));

            match analyze_xml(&text) {
                Ok(metrics) => {
                    if imp.lbl_inspector_status.label().as_str().starts_with("Error:") {
                        imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                        imp.lbl_inspector_status.set_tooltip_text(None);
                    }

                    imp.lbl_syntax_valid.set_label(tr!("✓ XML Válido", "✓ Valid XML"));
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_rfc_badge.set_label(tr!("Válido W3C XML", "W3C XML Valid"));
                    imp.lbl_rfc_badge.remove_css_class("badge-error");
                    imp.lbl_rfc_badge.add_css_class("badge-success");

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

                    imp.lbl_rfc_badge.set_label(tr!("Error sintáctico", "Syntax error"));
                    imp.lbl_rfc_badge.remove_css_class("badge-success");
                    imp.lbl_rfc_badge.add_css_class("badge-error");

                    imp.lbl_inspector_status.set_label(&err_msg);
                    imp.lbl_inspector_status.set_tooltip_text(Some(&err_msg));
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

            if parse_vardump(&text).is_ok() {
                imp.lbl_syntax_valid.set_label(tr!("✓ Array raw Válido", "✓ Valid Array raw"));
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_rfc_badge.set_label(tr!("print_r PHP Listo", "PHP print_r Ready"));
                imp.lbl_rfc_badge.remove_css_class("badge-error");
                imp.lbl_rfc_badge.add_css_class("badge-success");

                let kb = text.len() as f64 / 1024.0;
                imp.lbl_payload_size.set_label(&format!("{:.1} KB", kb));
                imp.lbl_tree_stats.set_label(tr!(
                    "Estructura PHP print_r / Array raw detectada",
                    "PHP print_r / Array raw structure detected"
                ));
            } else {
                imp.lbl_syntax_valid.set_label(tr!("✗ Array raw Inválido", "✗ Invalid Array raw"));
                imp.lbl_syntax_valid.remove_css_class("badge-success");
                imp.lbl_syntax_valid.add_css_class("badge-error");

                imp.lbl_rfc_badge.set_label(tr!("Error sintáctico", "Syntax error"));
                imp.lbl_rfc_badge.remove_css_class("badge-success");
                imp.lbl_rfc_badge.add_css_class("badge-error");
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

            if parse_vardump(&text).is_ok() {
                imp.lbl_syntax_valid.set_label(tr!("✓ var_dump Válido", "✓ Valid var_dump"));
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_rfc_badge.set_label(tr!("AST PHP Listo", "PHP AST Ready"));
                imp.lbl_rfc_badge.remove_css_class("badge-error");
                imp.lbl_rfc_badge.add_css_class("badge-success");

                let kb = text.len() as f64 / 1024.0;
                imp.lbl_payload_size.set_label(&format!("{:.1} KB", kb));
                imp.lbl_tree_stats.set_label(tr!(
                    "Estructura PHP var_dump detectada",
                    "PHP var_dump structure detected"
                ));
            } else {
                imp.lbl_syntax_valid.set_label(tr!("✗ var_dump Inválido", "✗ Invalid var_dump"));
                imp.lbl_syntax_valid.remove_css_class("badge-success");
                imp.lbl_syntax_valid.add_css_class("badge-error");

                imp.lbl_rfc_badge.set_label(tr!("Error sintáctico", "Syntax error"));
                imp.lbl_rfc_badge.remove_css_class("badge-success");
                imp.lbl_rfc_badge.add_css_class("badge-error");
            }
            if imp.btn_insp_view_tree.is_active() {
                self.refresh_tree_view();
            }
            return;
        }

        match analyze_json(&text) {
            Ok(metrics) => {
                if imp.lbl_inspector_status.label().as_str().starts_with("Error:") {
                    imp.lbl_inspector_status.set_label(tr!("Listo", "Ready"));
                    imp.lbl_inspector_status.set_tooltip_text(None);
                }

                imp.btn_format_json.set_tooltip_text(Some(tr!("Formatear JSON (Ctrl+Shift+F)", "Format JSON (Ctrl+Shift+F)")));
                imp.btn_minify_json.set_tooltip_text(Some(tr!("Minificar JSON", "Minify JSON")));
                imp.btn_vardump_to_json.set_tooltip_text(Some(tr!("Convertir a JSON", "Convert to JSON")));

                imp.lbl_syntax_valid.set_label(tr!("✓ Sintaxis Válida", "✓ Valid Syntax"));
                imp.lbl_syntax_valid.remove_css_class("badge-error");
                imp.lbl_syntax_valid.add_css_class("badge-success");

                imp.lbl_rfc_badge.set_label(tr!("Válido RFC 8259", "RFC 8259 Valid"));
                imp.lbl_rfc_badge.remove_css_class("badge-error");
                imp.lbl_rfc_badge.add_css_class("badge-success");

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

                imp.lbl_rfc_badge.set_label(&err_loc);
                imp.lbl_rfc_badge.remove_css_class("badge-success");
                imp.lbl_rfc_badge.add_css_class("badge-error");
            }
        }

        if imp.btn_insp_view_tree.is_active() {
            self.refresh_tree_view();
        }
    }

    fn switch_to_tree_view(&self) {
        let imp = self.imp();
        imp.stack_inspector_view.set_visible_child_name("page_tree");
        self.refresh_tree_view();
    }

    fn refresh_tree_view(&self) {
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
        imp.lbl_footer_diff_badge
            .set_label(&format!("+{}  -{}", summary.insertions, summary.deletions));

        imp.hunk_positions.replace(summary.hunk_positions);
        imp.last_unified_diff.replace(summary.unified_diff);
        imp.current_hunk_idx.set(0);
    }

    fn handle_load_demo(&self) {
        let imp = self.imp();
        let active = imp.main_stack.visible_child_name().unwrap_or_default();

        if active == "diff_page" {
            let demo_left = r#"{
  "service": "payment-gateway",
  "version": "2.4.0",
  "environment": "staging",
  "booking": {
    "confirmation_code": "J9MYX3",
    "status": "pending",
    "amount_usd": 149.50,
    "retry_limit": 3
  },
  "features": [
    "webhook_v1",
    "fraud_check"
  ]
}"#;
            let demo_right = r#"{
  "service": "payment-gateway",
  "version": "2.5.0",
  "environment": "production",
  "booking": {
    "confirmation_code": "9R99XK",
    "status": "confirmed",
    "amount_usd": 185.00,
    "retry_limit": 5,
    "instant_settlement": true
  },
  "features": [
    "webhook_v2",
    "fraud_check",
    "idempotency_guard"
  ]
}"#;
            imp.source_diff_left.buffer().set_text(demo_left);
            imp.source_diff_right.buffer().set_text(demo_right);
            imp.lbl_left_file.set_label("staging-v2.4.json");
            imp.lbl_right_file.set_label("production-v2.5.json");
            self.handle_compute_diff();
        } else {
            let demo_inspector = if imp.btn_mode_xml.is_active() {
                r#"<?xml version="1.0" encoding="UTF-8"?>
<catalog environment="production">
  <service id="srv-101" region="us-east-1">
    <name>VarLens AST Engine</name>
    <status>healthy</status>
    <latency_ms>1.8</latency_ms>
    <endpoints>
      <endpoint method="POST" auth="oauth2">/v1/inspect</endpoint>
      <endpoint method="POST" auth="oauth2">/v1/diff</endpoint>
    </endpoints>
  </service>
</catalog>"#
            } else if imp.btn_mode_vardump.is_active() {
                r#"object(App\Domain\UserSession)#42 (4) {
  ["user_id"]=>
  int(90841)
  ["username"]=>
  string(12) "brayan_dev"
  ["is_verified"]=>
  bool(true)
  ["permissions":protected]=>
  array(3) {
    [0]=>
    string(10) "json:write"
    [1]=>
    string(9) "diff:read"
    [2]=>
    string(11) "system:root"
  }
}"#
            } else if imp.btn_mode_raw.is_active() {
                r#"Array
(
    [cluster] => eu-central-1
    [nodes_online] => 8
    [maintenance] => false
    [replicas] => Array
        (
            [0] => node-alpha.internal
            [1] => node-beta.internal
        )
)"#
            } else {
                r#"{
  "application": "VarLens",
  "release": "1.1.1",
  "engine": {
    "language": "Rust 2021",
    "ui_toolkit": "GTK4 + Libadwaita",
    "zero_cloud_telemetry": true,
    "supported_formats": [
      "RFC 8259 JSON",
      "W3C XML",
      "PHP var_dump()",
      "PHP print_r() & var_export()"
    ]
  },
  "metrics": {
    "startup_ms": 14.2,
    "fps_target": 60,
    "documentation": "https://github.com/PinedaMB/VarLens"
  }
}"#
            };
            imp.source_inspector.buffer().set_text(demo_inspector);
            imp.lbl_inspector_file.set_label("demo-sample");
            imp.lbl_inspector_status.set_label(tr!(
                "Datos de demostración cargados",
                "Demo sample loaded"
            ));
        }
    }

    fn setup_code_search(&self) {
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

    fn setup_drag_and_drop(&self) {
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

    fn load_gio_file(&self, file: &gio::File, target: FileTarget) {
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

    fn handle_save_file(&self) {
        let imp = self.imp();
        let active = imp.main_stack.visible_child_name().unwrap_or_default();
        let (text, default_name) = if active == "diff_page" {
            let right_buf = imp.source_diff_right.buffer();
            let r_text = right_buf
                .text(&right_buf.start_iter(), &right_buf.end_iter(), true)
                .to_string();
            if !r_text.trim().is_empty() {
                (r_text, "modified.json")
            } else {
                let left_buf = imp.source_diff_left.buffer();
                (
                    left_buf
                        .text(&left_buf.start_iter(), &left_buf.end_iter(), true)
                        .to_string(),
                    "original.json",
                )
            }
        } else {
            let buf = imp.source_inspector.buffer();
            let ext = if imp.btn_mode_xml.is_active() {
                "payload.xml"
            } else {
                "payload.json"
            };
            (
                buf.text(&buf.start_iter(), &buf.end_iter(), true)
                    .to_string(),
                ext,
            )
        };

        if text.trim().is_empty() {
            imp.lbl_inspector_status
                .set_label(tr!("El editor está vacío", "Editor is empty"));
            return;
        }

        let file_dialog = gtk4::FileDialog::builder()
            .title(tr!("Guardar archivo", "Save file"))
            .initial_name(default_name)
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

    fn normalize_single_payload(text: &str, indent: IndentStyle) -> Option<String> {
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
        if let Ok(vd_json) = vardump_to_json_string(trimmed, true) {
            if let Ok(sorted) = sort_json_keys(&vd_json, indent) {
                return Some(sorted);
            }
            return Some(vd_json);
        }
        None
    }

    fn handle_normalize_diff(&self) {
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

        if let Some(norm_l) = Self::normalize_single_payload(&left_text, indent) {
            left_buf.set_text(&norm_l);
        }
        if let Some(norm_r) = Self::normalize_single_payload(&right_text, indent) {
            right_buf.set_text(&norm_r);
        }
        self.handle_compute_diff();
    }

    fn handle_sort_json_keys(&self) {
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

    fn handle_json_format(&self, pretty: bool) {
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

        let action_name = if is_xml {
            if pretty {
                tr!("Formateando XML", "Formatting XML")
            } else {
                tr!("Minificando XML", "Minifying XML")
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
                            is_xml: true,
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
                            is_xml: false,
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

    fn handle_vardump_convert(&self) {
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
                        is_xml: false,
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

    fn handle_worker_result(&self, result: WorkerResult) {
        let imp = self.imp();
        match result {
            WorkerResult::InspectorFormatted {
                text,
                message,
                is_xml,
            } => {
                let buf = imp.source_inspector.buffer();
                buf.set_text(&text);
                if is_xml {
                    Self::set_buffer_language(&buf, "xml");
                    imp.btn_mode_xml.set_active(true);
                } else {
                    Self::set_buffer_language(&buf, "json");
                    imp.btn_mode_json.set_active(true);
                }
                imp.lbl_inspector_status.set_label(&message);
                imp.lbl_inspector_status.set_tooltip_text(None);
            }
            WorkerResult::DiffCalculated(summary) => {
                self.apply_diff_summary(summary);
            }
            WorkerResult::Error(err) => {
                imp.lbl_inspector_status
                    .set_label(&format!("Error: {}", err));
                imp.lbl_inspector_status.set_tooltip_text(Some(&err));
            }
        }
    }

    fn show_help_dialog(&self) {
        let win = adw::Window::builder()
            .title(tr!("Atajos de Teclado y Detalles", "Keyboard Shortcuts &amp; Details"))
            .modal(true)
            .transient_for(self)
            .default_width(620)
            .default_height(640)
            .build();

        let toolbar_view = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        toolbar_view.add_top_bar(&header);

        let content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 20);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);
        content_box.set_margin_top(16);
        content_box.set_margin_bottom(24);

        // --- 1. Grupo Atajos del Teclado ---
        let grp_shortcuts = adw::PreferencesGroup::builder()
            .title(tr!("Atajos del Teclado", "Keyboard Shortcuts"))
            .description(tr!("Accesos rápidos para aumentar tu productividad", "Quick keys to boost your productivity"))
            .build();

        let make_shortcut_row = |title: &str, subtitle: Option<&str>, accel: &str| {
            let row = adw::ActionRow::builder()
                .title(title)
                .build();
            if let Some(sub) = subtitle {
                row.set_subtitle(sub);
            }
            let label = gtk4::ShortcutLabel::builder()
                .accelerator(accel)
                .valign(gtk4::Align::Center)
                .build();
            row.add_suffix(&label);
            row
        };

        grp_shortcuts.add(&make_shortcut_row(
            tr!("Abrir archivo", "Open file"),
            Some(tr!("Carga un archivo en el panel activo", "Load a file into active panel")),
            "<Control>O",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Guardar archivo", "Save file"),
            Some(tr!("Guarda el contenido del editor en disco", "Save editor content to disk")),
            "<Control>S",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Buscar en código", "Search in code"),
            Some(tr!("Enfoca la barra de búsqueda del editor", "Focus the editor search bar")),
            "<Control>F",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Formatear documento", "Format document"),
            Some(tr!("Indentación automática de JSON o XML", "Automatic JSON or XML indentation")),
            "<Control><Shift>F",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Minificar documento", "Minify document"),
            Some(tr!("Compacta el contenido a una sola línea", "Compact content into a single line")),
            "<Control><Shift>M",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Ordenar claves JSON", "Sort JSON keys"),
            Some(tr!("Ordena recursivamente todas las claves (A→Z)", "Recursively sort all object keys (A→Z)")),
            "<Control><Shift>S",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Convertir PHP var_dump / XML", "Convert PHP var_dump / XML"),
            Some(tr!("Transforma volcados de PHP o XML a JSON", "Transform PHP dumps or XML into JSON")),
            "<Control><Shift>J",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Recalcular diferencias", "Recalculate diff"),
            Some(tr!("Actualiza la comparación entre ambos paneles", "Update comparison between diff panels")),
            "<Control>Return",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Diferencia anterior / siguiente", "Previous / Next diff"),
            Some(tr!("Salta entre bloques de diferencias", "Jump between diff chunks")),
            "<Control>bracketleft",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Limpiar paneles de comparación", "Clear diff panels"),
            None,
            "<Control><Shift>C",
        ));
        grp_shortcuts.add(&make_shortcut_row(
            tr!("Ver atajos y ayuda", "Show shortcuts &amp; help"),
            None,
            "F1",
        ));

        content_box.append(&grp_shortcuts);

        // --- 2. Grupo Detalles y Capacidades ---
        let grp_features = adw::PreferencesGroup::builder()
            .title(tr!("Detalles y Capacidades", "Features &amp; Details"))
            .description(tr!("Puntos destacados de la arquitectura de VarLens", "Core architecture highlights of VarLens"))
            .build();

        let make_feature_row = |icon_name: &str, title: &str, subtitle: &str| {
            let row = adw::ActionRow::builder()
                .title(title)
                .subtitle(subtitle)
                .build();
            let icon = gtk4::Image::from_icon_name(icon_name);
            icon.set_icon_size(gtk4::IconSize::Large);
            row.add_prefix(&icon);
            row
        };

        grp_features.add(&make_feature_row(
            "document-properties-symbolic",
            tr!("Inspector de Datos", "Data Inspector"),
            tr!("Validación RFC 8259 y W3C en tiempo real con conteo de nodos y profundidad.", "Real-time RFC 8259 &amp; W3C validation with node counts and depth."),
        ));
        grp_features.add(&make_feature_row(
            "format-indent-more-symbolic",
            tr!("Explorador de Árbol", "Tree Explorer"),
            tr!("Navegación jerárquica con búsqueda interactiva y copia instantánea de rutas de acceso.", "Interactive hierarchical navigation with query filtering and 1-click path copy."),
        ));
        grp_features.add(&make_feature_row(
            "text-x-generic-symbolic",
            tr!("Parser PHP PEG", "PHP PEG Parser"),
            tr!("Gramática robusta para arrays indexados, asociativos y objetos de var_dump() y print_r().", "Robust grammar for indexed/associative arrays and objects from var_dump() &amp; print_r()."),
        ));
        grp_features.add(&make_feature_row(
            "edit-find-symbolic",
            tr!("Comparador Visual", "Visual Comparator"),
            tr!("Algoritmos Myers y Patience con resaltado de tokens a nivel de caracteres.", "Myers &amp; Patience algorithms with character-level token highlighting."),
        ));
        grp_features.add(&make_feature_row(
            "security-high-symbolic",
            tr!("Privacidad Total", "Zero-Cloud Privacy"),
            tr!("Todo el procesamiento es 100% local en tu equipo, sin llamadas de red ni telemetría.", "100% local processing on your machine with zero network calls or telemetry."),
        ));

        content_box.append(&grp_features);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&content_box)
            .build();

        toolbar_view.set_content(Some(&scrolled));
        win.set_content(Some(&toolbar_view));
        win.present();
    }
}
