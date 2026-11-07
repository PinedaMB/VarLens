use crate::core::diff::{compute_diff_ext, DiffSummary};
use crate::core::json::{
    analyze_json, count_lines, format_json_with_indent, IndentStyle,
};
use crate::core::vardump::{parse_vardump, vardump_to_json_string};
use crate::ui::editor::EditorView;
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

const SAMPLE_DIFF_OLD: &str = r#"{
  "service_name": "vardiff-core",
  "version": "1.4.0",
  "environment": "production",
  "config": {
    "debug_mode": true,
    "workers_count": 8,
    "cache_driver": "redis_sentinel",
    "max_payload_mb": 64,
    "network_profile": {
      "timeout_seconds": 30,
      "keep_alive": true,
      "tls_enabled": true
    }
  }
}"#;

const SAMPLE_DIFF_NEW: &str = r#"{
  "service_name": "vardiff-core",
  "version": "1.5.0",
  "environment": "production",
  "config": {
    "debug_mode": false,
    "workers_count": 8,
    "cache_driver": "redis_sentinel",
    "max_payload_mb": 64,
    "network_profile": {
      "timeout_seconds": 60,
      "retry_policy": {"max": 3, "backoff": "exponential"},
      "circuit_breaker": true,
      "keep_alive": true,
      "tls_enabled": true
    }
  }
}"#;

const SAMPLE_JSON: &str = r#"{
  "id": "usr_9f83a21",
  "actor": {
    "username": "miguel.valenzuela",
    "role": "systems-architect",
    "active": true,
    "quota_limit": 4096,
    "deleted_at": null
  },
  "execution_context": {
    "runtime": "linux-x86_64-gnome46",
    "pid": 28419,
    "memory_mb": 128.45,
    "privileged": false
  },
  "ast_transform_rules": [
    {
      "target": "PHP_XDEBUG_DUMP",
      "strip_depth_tags": true,
      "cast_numeric_strings": true,
      "resolve_references": true
    },
    {
      "target": "FORMATTED_JSON",
      "sort_keys_alphabetical": false,
      "escape_slashes": false
    }
  ]
}"#;

const SAMPLE_VARDUMP: &str = r#"object(App\Models\User)#42 (6) {
  ["id"]=>
  string(11) "usr_9f83a21"
  ["username"]=>
  string(17) "miguel.valenzuela"
  ["role"]=>
  string(17) "systems-architect"
  ["active"]=>
  bool(true)
  ["roles"]=>
  array(3) {
    [0]=>
    string(5) "admin"
    [1]=>
    string(9) "architect"
    [2]=>
    string(7) "manager"
  }
  ["execution_context"]=>
  array(3) {
    ["runtime"]=>
    string(20) "linux-x86_64-gnome46"
    ["pid"]=>
    int(28419)
    ["memory_mb"]=>
    float(128.45)
  }
  ["api_key":"App\Models\User":private]=>
  string(32) "9b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e"
}"#;

enum WorkerResult {
    InspectorFormatted { text: String, message: String },
    DiffCalculated(DiffSummary),
    Error(String),
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
        pub source_diff_left: TemplateChild<sourceview5::View>,
        #[template_child]
        pub source_diff_right: TemplateChild<sourceview5::View>,
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
        pub btn_copy_inspector: TemplateChild<gtk4::Button>,
        #[template_child]
        pub btn_mode_json: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_vardump: TemplateChild<gtk4::ToggleButton>,
        #[template_child]
        pub btn_mode_raw: TemplateChild<gtk4::ToggleButton>,
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
            .background_rgba(&gdk::RGBA::new(0.88, 0.11, 0.14, 0.18))
            .paragraph_background_rgba(&gdk::RGBA::new(0.88, 0.11, 0.14, 0.16))
            .build();

        let tag_ins = gtk4::TextTag::builder()
            .name("diff-inserted")
            .background_rgba(&gdk::RGBA::new(0.18, 0.76, 0.49, 0.18))
            .paragraph_background_rgba(&gdk::RGBA::new(0.18, 0.76, 0.49, 0.16))
            .build();

        let tag_span_del = gtk4::TextTag::builder()
            .name("diff-span-deleted")
            .background_rgba(&gdk::RGBA::new(0.92, 0.18, 0.22, 0.48))
            .weight(700)
            .build();

        let tag_span_ins = gtk4::TextTag::builder()
            .name("diff-span-inserted")
            .background_rgba(&gdk::RGBA::new(0.20, 0.82, 0.52, 0.48))
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

        // 6. Register Window Actions & Shortcuts
        self.setup_actions();
        self.setup_shortcuts();

        // 7. Populate Default Payloads matching Mockups
        left_buf.set_text(SAMPLE_DIFF_OLD);
        right_buf.set_text(SAMPLE_DIFF_NEW);
        inspector_buf.set_text(SAMPLE_JSON);

        self.handle_compute_diff();
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
                imp.lbl_inspector_file.set_label("schema/payload_v2.json");
                win_m_json.update_inspector_metrics();
            }
        });

        let win_m_vd = self.clone();
        imp.btn_mode_vardump.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_vd.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "php");
                imp.lbl_inspector_file.set_label("dump/xdebug_output.dump");
                let buf = imp.source_inspector.buffer();
                if buf.char_count() == 0 || analyze_json(&buf.text(&buf.start_iter(), &buf.end_iter(), true)).is_ok() {
                    buf.set_text(SAMPLE_VARDUMP);
                }
                win_m_vd.update_inspector_metrics();
            }
        });

        let win_m_raw = self.clone();
        imp.btn_mode_raw.connect_toggled(move |btn| {
            if btn.is_active() {
                let imp = win_m_raw.imp();
                Self::set_buffer_language(&imp.source_inspector.buffer(), "json");
                imp.lbl_inspector_file.set_label("raw/array_payload.txt");
                win_m_raw.update_inspector_metrics();
            }
        });

        // Inspector buffer changes -> live AST metrics & validation
        let win_ibuf = self.clone();
        imp.source_inspector.buffer().connect_changed(move |_| {
            win_ibuf.update_inspector_metrics();
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
            self_open_l.handle_open_file(true);
        });
        self.add_action(&act_open_l);

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
                imp.btn_copy_inspector.set_label("✓ Copiado");
                let btn_clone = imp.btn_copy_inspector.clone();
                glib::timeout_add_local_once(Duration::from_millis(1800), move || {
                    btn_clone.set_label("Copiar");
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

        // Samples: var_dump
        let self_svd = self.clone();
        let act_svd = gio::SimpleAction::new("sample-vardump", None);
        act_svd.connect_activate(move |_, _| {
            let imp = self_svd.imp();
            imp.btn_mode_vardump.set_active(true);
            imp.source_inspector.buffer().set_text(SAMPLE_VARDUMP);
            imp.main_stack.set_visible_child_name("inspector_page");
            imp.lbl_inspector_status.set_label("Ejemplo de var_dump cargado");
        });
        self.add_action(&act_svd);

        // Samples: JSON
        let self_sjs = self.clone();
        let act_sjs = gio::SimpleAction::new("sample-json", None);
        act_sjs.connect_activate(move |_, _| {
            let imp = self_sjs.imp();
            imp.btn_mode_json.set_active(true);
            imp.source_inspector.buffer().set_text(SAMPLE_JSON);
            imp.main_stack.set_visible_child_name("inspector_page");
            imp.lbl_inspector_status.set_label("Ejemplo de JSON cargado");
        });
        self.add_action(&act_sjs);

        // Samples: Diff
        let self_sdf = self.clone();
        let act_sdf = gio::SimpleAction::new("sample-diff", None);
        act_sdf.connect_activate(move |_, _| {
            let imp = self_sdf.imp();
            imp.source_diff_left.buffer().set_text(SAMPLE_DIFF_OLD);
            imp.source_diff_right.buffer().set_text(SAMPLE_DIFF_NEW);
            imp.main_stack.set_visible_child_name("diff_page");
            self_sdf.handle_compute_diff();
        });
        self.add_action(&act_sdf);
    }

    fn update_inspector_metrics(&self) {
        let imp = self.imp();
        let buf = imp.source_inspector.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), true).to_string();

        if text.trim().is_empty() {
            imp.lbl_syntax_valid.set_label("Editor vacío");
            imp.lbl_node_count.set_label("0 nodos");
            imp.lbl_payload_size.set_label("0 B");
            imp.lbl_tree_stats.set_label("Árbol JSON: 0 niveles | 0 claves");
            return;
        }

        match analyze_json(&text) {
            Ok(metrics) => {
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
                    "Árbol JSON: {} niveles | {} claves",
                    metrics.max_depth, metrics.total_keys
                ));
            }
            Err(err_loc) => {
                if parse_vardump(&text).is_ok() {
                    imp.lbl_syntax_valid.set_label("✓ var_dump Válido");
                    imp.lbl_syntax_valid.remove_css_class("badge-error");
                    imp.lbl_syntax_valid.add_css_class("badge-success");

                    imp.lbl_rfc_badge.set_label("AST PHP Listo");
                    imp.lbl_rfc_badge.remove_css_class("badge-error");
                    imp.lbl_rfc_badge.add_css_class("badge-success");

                    let kb = text.len() as f64 / 1024.0;
                    imp.lbl_payload_size.set_label(&format!("{:.1} KB", kb));
                    imp.lbl_tree_stats
                        .set_label("Estructura PHP var_dump detectada");
                } else {
                    imp.lbl_syntax_valid.set_label("✗ Sintaxis Inválida");
                    imp.lbl_syntax_valid.remove_css_class("badge-success");
                    imp.lbl_syntax_valid.add_css_class("badge-error");

                    imp.lbl_rfc_badge.set_label(&err_loc);
                    imp.lbl_rfc_badge.remove_css_class("badge-success");
                    imp.lbl_rfc_badge.add_css_class("badge-error");
                }
            }
        }
    }

    fn handle_open_file(&self, force_left_diff: bool) {
        let file_dialog = gtk4::FileDialog::builder()
            .title("Abrir archivo para inspección o comparación")
            .modal(true)
            .build();

        let self_clone = self.clone();
        file_dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                let filename = file
                    .basename()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "archivo.json".to_string());

                if let Ok((contents, _)) = file.load_contents(gio::Cancellable::NONE) {
                    if let Ok(text) = String::from_utf8(contents.to_vec()) {
                        let imp = self_clone.imp();
                        let active = imp.main_stack.visible_child_name().unwrap_or_default();
                        if force_left_diff {
                            imp.source_diff_left.buffer().set_text(&text);
                            self_clone.handle_compute_diff();
                        } else if active == "diff_page" {
                            let left_buf = imp.source_diff_left.buffer();
                            let right_buf = imp.source_diff_right.buffer();
                            if left_buf.char_count() == 0 {
                                left_buf.set_text(&text);
                            } else {
                                right_buf.set_text(&text);
                                self_clone.handle_compute_diff();
                            }
                        } else {
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
        });
    }

    fn clear_diff_highlights(&self) {
        let imp = self.imp();
        let left_buf = imp.source_diff_left.buffer();
        let right_buf = imp.source_diff_right.buffer();

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

        let action_name = if pretty {
            "Formateando JSON"
        } else {
            "Minificando JSON"
        };

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status
                .set_label(&format!("{} ({} líneas)...", action_name, lines));
            let handles = glib::thread_guard::ThreadGuard::new(self.clone());

            thread::spawn(move || {
                let res = match format_json_with_indent(&text, pretty, indent) {
                    Ok(formatted) => WorkerResult::InspectorFormatted {
                        text: formatted,
                        message: if pretty {
                            "JSON formateado correctamente".to_string()
                        } else {
                            "JSON minificado correctamente".to_string()
                        },
                    },
                    Err(e) => WorkerResult::Error(e),
                };
                glib::idle_add_once(move || {
                    handles.into_inner().handle_worker_result(res);
                });
            });
        } else {
            match format_json_with_indent(&text, pretty, indent) {
                Ok(formatted) => {
                    buf.set_text(&formatted);
                    imp.btn_mode_json.set_active(true);
                    imp.lbl_inspector_status.set_label(if pretty {
                        "JSON formateado"
                    } else {
                        "JSON minificado"
                    });
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(&format!("Error: {}", err));
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
            return;
        }

        if lines > ASYNC_LINE_THRESHOLD {
            imp.lbl_inspector_status
                .set_label(&format!("Parseando var_dump ({} líneas)...", lines));
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
                    imp.btn_mode_json.set_active(true);
                    buf.set_text(&json_str);
                    imp.lbl_inspector_status
                        .set_label("var_dump convertido a JSON");
                }
                Err(err) => {
                    imp.lbl_inspector_status
                        .set_label(&format!("Error: {}", err));
                }
            }
        }
    }

    fn handle_worker_result(&self, result: WorkerResult) {
        let imp = self.imp();
        match result {
            WorkerResult::InspectorFormatted { text, message } => {
                imp.btn_mode_json.set_active(true);
                imp.source_inspector.buffer().set_text(&text);
                imp.lbl_inspector_status.set_label(&message);
            }
            WorkerResult::DiffCalculated(summary) => {
                self.apply_diff_summary(summary);
            }
            WorkerResult::Error(err) => {
                imp.lbl_inspector_status
                    .set_label(&format!("Error: {}", err));
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
