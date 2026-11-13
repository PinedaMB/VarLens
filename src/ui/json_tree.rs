use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use crate::tr;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonNodeType {
    Object(usize),
    Array(usize),
    String(String),
    Number(String),
    Boolean(bool),
    Null,
}

mod imp_node {
    use super::*;

    #[derive(Default)]
    pub struct JsonNodeObject {
        pub key: RefCell<String>,
        pub node_type: RefCell<Option<JsonNodeType>>,
        pub path: RefCell<String>,
        pub access_path: RefCell<String>,
        pub raw_value: RefCell<String>,
        pub json_value: RefCell<String>,
        pub decoded_kind: RefCell<Option<crate::core::decoder::DecodedKind>>,
        pub children: RefCell<Option<gio::ListStore>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for JsonNodeObject {
        const NAME: &'static str = "VarLensJsonNodeObject";
        type Type = super::JsonNodeObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for JsonNodeObject {}
}

glib::wrapper! {
    pub struct JsonNodeObject(ObjectSubclass<imp_node::JsonNodeObject>);
}

fn is_simple_identifier(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn format_child_access_path(parent: &str, key: &str) -> String {
    if is_simple_identifier(key) {
        if parent.is_empty() {
            key.to_string()
        } else {
            format!("{}.{}", parent, key)
        }
    } else {
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{}[\"{}\"]", parent, escaped)
    }
}

impl JsonNodeObject {
    pub fn new(key: &str, value: &serde_json::Value, path: &str, access_path: &str) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        *imp.key.borrow_mut() = key.to_string();
        *imp.path.borrow_mut() = path.to_string();
        *imp.access_path.borrow_mut() = access_path.to_string();

        match value {
            serde_json::Value::Object(map) => {
                *imp.node_type.borrow_mut() = Some(JsonNodeType::Object(map.len()));
                *imp.raw_value.borrow_mut() = format!("{} keys", map.len());
                *imp.json_value.borrow_mut() =
                    serde_json::to_string_pretty(value).unwrap_or_default();
                let store = gio::ListStore::new::<JsonNodeObject>();
                for (k, v) in map {
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{} ▶ {}", path, k)
                    };
                    let child_access = format_child_access_path(access_path, k);
                    let child = JsonNodeObject::new(k, v, &child_path, &child_access);
                    store.append(&child);
                }
                *imp.children.borrow_mut() = Some(store);
            }
            serde_json::Value::Array(arr) => {
                *imp.node_type.borrow_mut() = Some(JsonNodeType::Array(arr.len()));
                *imp.raw_value.borrow_mut() = format!("{} items", arr.len());
                *imp.json_value.borrow_mut() =
                    serde_json::to_string_pretty(value).unwrap_or_default();
                let store = gio::ListStore::new::<JsonNodeObject>();
                for (i, v) in arr.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        i.to_string()
                    } else {
                        format!("{} ▶ {}", path, i)
                    };
                    let child_access = format!("{}[{}]", access_path, i);
                    let child = JsonNodeObject::new(&i.to_string(), v, &child_path, &child_access);
                    store.append(&child);
                }
                *imp.children.borrow_mut() = Some(store);
            }
            serde_json::Value::String(s) => {
                *imp.node_type.borrow_mut() = Some(JsonNodeType::String(s.clone()));
                *imp.raw_value.borrow_mut() = s.clone();
                *imp.json_value.borrow_mut() = s.clone();
                if let Some(decoded) = crate::core::decoder::detect_and_decode_str(s) {
                    *imp.decoded_kind.borrow_mut() = Some(decoded.kind);
                    let store = gio::ListStore::new::<JsonNodeObject>();
                    let sub_path = if path.is_empty() {
                        decoded.kind.badge_label().to_string()
                    } else {
                        format!("{} ▶ {}", path, decoded.kind.badge_label())
                    };
                    match &decoded.value {
                        serde_json::Value::Object(map) => {
                            for (k, v) in map {
                                let cp = format!("{} ▶ {}", sub_path, k);
                                let ca = format_child_access_path(access_path, k);
                                store.append(&JsonNodeObject::new(k, v, &cp, &ca));
                            }
                        }
                        serde_json::Value::Array(arr) => {
                            for (i, v) in arr.iter().enumerate() {
                                let cp = format!("{} ▶ {}", sub_path, i);
                                let ca = format!("{}[{}]", access_path, i);
                                store.append(&JsonNodeObject::new(&i.to_string(), v, &cp, &ca));
                            }
                        }
                        other => {
                            store.append(&JsonNodeObject::new(
                                "decoded",
                                other,
                                &sub_path,
                                access_path,
                            ));
                        }
                    }
                    *imp.children.borrow_mut() = Some(store);
                }
            }
            serde_json::Value::Number(n) => {
                let s = n.to_string();
                *imp.node_type.borrow_mut() = Some(JsonNodeType::Number(s.clone()));
                *imp.raw_value.borrow_mut() = s.clone();
                *imp.json_value.borrow_mut() = s;
            }
            serde_json::Value::Bool(b) => {
                *imp.node_type.borrow_mut() = Some(JsonNodeType::Boolean(*b));
                let s = b.to_string();
                *imp.raw_value.borrow_mut() = s.clone();
                *imp.json_value.borrow_mut() = s;
            }
            serde_json::Value::Null => {
                *imp.node_type.borrow_mut() = Some(JsonNodeType::Null);
                *imp.raw_value.borrow_mut() = "null".to_string();
                *imp.json_value.borrow_mut() = "null".to_string();
            }
        }

        obj
    }

    pub fn key(&self) -> String {
        self.imp().key.borrow().clone()
    }

    pub fn path(&self) -> String {
        self.imp().path.borrow().clone()
    }

    pub fn access_path(&self) -> String {
        self.imp().access_path.borrow().clone()
    }

    pub fn raw_value(&self) -> String {
        self.imp().raw_value.borrow().clone()
    }

    pub fn json_value(&self) -> String {
        self.imp().json_value.borrow().clone()
    }

    pub fn children(&self) -> Option<gio::ListStore> {
        self.imp().children.borrow().clone()
    }

    pub fn format_markup(&self, is_dark: bool) -> String {
        let imp = self.imp();
        let key = imp.key.borrow();
        let key_escaped = glib::markup_escape_text(&key);

        let (key_col, str_col, num_col, bool_col, dim_col, badge_bg) = if is_dark {
            (
                "#5eead4",
                "#fed7aa",
                "#ff7b72",
                "#34d399",
                "#8b949e",
                "#21262d",
            )
        } else {
            (
                "#0550ae",
                "#0a3069",
                "#cf222e",
                "#1a7f37",
                "#6e7781",
                "#eaeef2",
            )
        };

        match imp.node_type.borrow().as_ref() {
            Some(JsonNodeType::Object(count)) => {
                format!(
                    "<span background='{}' foreground='{}' size='small' weight='bold'> {{}} </span> <b><span foreground='{}'>{}</span></b> <span foreground='{}' size='small'>({} keys)</span>",
                    badge_bg, key_col, key_col, key_escaped, dim_col, count
                )
            }
            Some(JsonNodeType::Array(count)) => {
                format!(
                    "<span background='{}' foreground='{}' size='small' weight='bold'> [] </span> <b><span foreground='{}'>{}</span></b> <span foreground='{}' size='small'>[{} items]</span>",
                    badge_bg, bool_col, key_col, key_escaped, dim_col, count
                )
            }
            Some(JsonNodeType::String(s)) => {
                let val_escaped = glib::markup_escape_text(s);
                if let Some(kind) = *imp.decoded_kind.borrow() {
                    format!(
                        "<span background='{}' foreground='#e5a50a' size='x-small' weight='bold'> {} </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><span foreground='{}'>\"{}\"</span>",
                        badge_bg, kind.badge_label(), key_col, key_escaped, dim_col, str_col, val_escaped
                    )
                } else if s.starts_with("http://") || s.starts_with("https://") {
                    format!(
                        "<span background='{}' foreground='#3584e4' size='x-small' weight='bold'> URL </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><span foreground='#3584e4' underline='single'>\"{}\"</span>",
                        badge_bg, key_col, key_escaped, dim_col, val_escaped
                    )
                } else {
                    format!(
                        "<span background='{}' foreground='{}' size='x-small' weight='bold'> STR </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><span foreground='{}'>\"{}\"</span>",
                        badge_bg, str_col, key_col, key_escaped, dim_col, str_col, val_escaped
                    )
                }
            }
            Some(JsonNodeType::Number(n)) => {
                let val_escaped = glib::markup_escape_text(n);
                format!(
                    "<span background='{}' foreground='{}' size='x-small' weight='bold'> NUM </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><b><span foreground='{}'>{}</span></b>",
                    badge_bg, num_col, key_col, key_escaped, dim_col, num_col, val_escaped
                )
            }
            Some(JsonNodeType::Boolean(b)) => {
                format!(
                    "<span background='{}' foreground='{}' size='x-small' weight='bold'> BOOL </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><b><span foreground='{}'>{}</span></b>",
                    badge_bg, bool_col, key_col, key_escaped, dim_col, bool_col, b
                )
            }
            Some(JsonNodeType::Null) => {
                format!(
                    "<span background='{}' foreground='{}' size='x-small' weight='bold'> NULL </span> <b><span foreground='{}'>{}</span></b><span foreground='{}'>: </span><i><span foreground='{}'>null</span></i>",
                    badge_bg, dim_col, key_col, key_escaped, dim_col, dim_col
                )
            }
            None => key_escaped.to_string(),
        }
    }
}

impl std::fmt::Debug for JsonTreeView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonTreeView").finish()
    }
}

#[allow(dead_code)]
pub struct JsonTreeView {
    container: gtk4::Box,
    list_view: gtk4::ListView,
    tree_list_model: gtk4::TreeListModel,
    selection_model: gtk4::SingleSelection,
    lbl_breadcrumb: gtk4::Label,
    search_entry: gtk4::SearchEntry,
    lbl_search_count: gtk4::Label,
    btn_expand_all: gtk4::Button,
    btn_collapse_all: gtk4::Button,
    btn_copy_path: gtk4::Button,
    btn_copy_value: gtk4::Button,
    btn_prev_match: gtk4::Button,
    btn_next_match: gtk4::Button,
    current_access_path: Rc<RefCell<String>>,
    current_node_value: Rc<RefCell<String>>,
    search_matches: Rc<RefCell<Vec<u32>>>,
    current_match_idx: Rc<RefCell<usize>>,
}

impl JsonTreeView {
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.set_hexpand(true);
        container.set_vexpand(true);

        // Top Toolbar & Access Path bar
        let top_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        top_box.add_css_class("tree-top-bar");

        let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        toolbar.add_css_class("sub-toolbar");
        toolbar.set_margin_start(4);
        toolbar.set_margin_end(4);
        toolbar.set_margin_top(4);
        toolbar.set_margin_bottom(4);

        let btn_expand_all = gtk4::Button::builder()
            .icon_name("view-fullscreen-symbolic")
            .tooltip_text(tr!("Desplegar y expandir todos los nodos", "Expand all nodes"))
            .build();

        let btn_collapse_all = gtk4::Button::builder()
            .icon_name("view-restore-symbolic")
            .tooltip_text(tr!("Colapsar y contraer todos los nodos", "Collapse all nodes"))
            .build();

        let btn_copy_path = gtk4::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text(tr!("Copiar ruta de acceso (ej. array[0].HotelCode)", "Copy access path (e.g. array[0].HotelCode)"))
            .build();

        let btn_copy_value = gtk4::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text(tr!("Copiar valor o sub-JSON del nodo", "Copy node value or sub-JSON"))
            .build();

        let sep = gtk4::Separator::new(gtk4::Orientation::Vertical);

        let search_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        search_box.set_hexpand(true);
        search_box.set_halign(gtk4::Align::End);

        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text(tr!("Buscar nodos o valores...", "Search nodes or values..."))
            .width_request(240)
            .build();

        let btn_prev_match = gtk4::Button::builder()
            .icon_name("go-up-symbolic")
            .tooltip_text(tr!("Coincidencia anterior", "Previous match"))
            .has_frame(false)
            .build();

        let btn_next_match = gtk4::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text(tr!("Siguiente coincidencia", "Next match"))
            .has_frame(false)
            .build();

        let lbl_search_count = gtk4::Label::builder()
            .label("")
            .css_classes(["dim-label", "caption"])
            .build();

        search_box.append(&lbl_search_count);
        search_box.append(&search_entry);
        search_box.append(&btn_prev_match);
        search_box.append(&btn_next_match);

        toolbar.append(&btn_expand_all);
        toolbar.append(&btn_collapse_all);
        toolbar.append(&btn_copy_path);
        toolbar.append(&btn_copy_value);
        toolbar.append(&sep);
        toolbar.append(&search_box);

        // Breadcrumb access path header
        let path_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        path_bar.add_css_class("tree-path-bar");
        path_bar.set_margin_start(12);
        path_bar.set_margin_end(12);
        path_bar.set_margin_top(4);
        path_bar.set_margin_bottom(6);

        let path_icon = gtk4::Image::from_icon_name("folder-symbolic");
        path_icon.add_css_class("dim-label");

        let lbl_breadcrumb = gtk4::Label::builder()
            .label("Selecciona un nodo para ver su ruta...")
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .xalign(0.0)
            .selectable(true)
            .hexpand(true)
            .css_classes(["caption", "font-mono"])
            .build();

        path_bar.append(&path_icon);
        path_bar.append(&lbl_breadcrumb);

        top_box.append(&toolbar);
        top_box.append(&path_bar);
        container.append(&top_box);

        // Root store and TreeListModel
        let root_store = gio::ListStore::new::<JsonNodeObject>();
        let tree_list_model = gtk4::TreeListModel::new(
            root_store,
            false,
            false,
            |item| {
                if let Some(node) = item.downcast_ref::<JsonNodeObject>() {
                    node.children().map(|store| store.upcast::<gio::ListModel>())
                } else {
                    None
                }
            },
        );

        let selection_model = gtk4::SingleSelection::new(Some(tree_list_model.clone()));
        selection_model.set_autoselect(false);

        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_factory, obj| {
            let list_item = obj.downcast_ref::<gtk4::ListItem>().unwrap();
            let expander = gtk4::TreeExpander::new();
            expander.set_indent_for_depth(true);
            expander.set_valign(gtk4::Align::Center);
            let label = gtk4::Label::builder()
                .xalign(0.0)
                .yalign(0.5)
                .use_markup(true)
                .selectable(true)
                .css_classes(["tree-node-label", "font-mono"])
                .build();
            expander.set_child(Some(&label));
            list_item.set_child(Some(&expander));
        });

        factory.connect_bind(|_factory, obj| {
            let list_item = obj.downcast_ref::<gtk4::ListItem>().unwrap();
            if let Some(tree_row) = list_item.item().and_downcast::<gtk4::TreeListRow>() {
                if let Some(expander) = list_item.child().and_downcast::<gtk4::TreeExpander>() {
                    expander.set_list_row(Some(&tree_row));
                    if let Some(node) = tree_row.item().and_downcast::<JsonNodeObject>() {
                        if let Some(label) = expander.child().and_downcast::<gtk4::Label>() {
                            let is_dark = libadwaita::StyleManager::default().is_dark();
                            label.set_markup(&node.format_markup(is_dark));
                        }
                    }
                }
            }
        });

        factory.connect_unbind(|_factory, obj| {
            let list_item = obj.downcast_ref::<gtk4::ListItem>().unwrap();
            if let Some(expander) = list_item.child().and_downcast::<gtk4::TreeExpander>() {
                expander.set_list_row(None::<&gtk4::TreeListRow>);
                if let Some(label) = expander.child().and_downcast::<gtk4::Label>() {
                    label.set_text("");
                }
            }
        });

        factory.connect_teardown(|_factory, obj| {
            let list_item = obj.downcast_ref::<gtk4::ListItem>().unwrap();
            list_item.set_child(None::<&gtk4::Widget>);
        });

        let list_view = gtk4::ListView::new(Some(selection_model.clone()), Some(factory));
        list_view.add_css_class("tree-list-view");

        // Allow clicking row to toggle expand/collapse
        let model_act = tree_list_model.clone();
        list_view.connect_activate(move |_lv, pos| {
            if let Some(row) = model_act.row(pos) {
                if row.is_expandable() {
                    row.set_expanded(!row.is_expanded());
                }
            }
        });

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&list_view)
            .build();

        container.append(&scrolled);

        let current_access_path = Rc::new(RefCell::new(String::new()));
        let current_node_value = Rc::new(RefCell::new(String::new()));
        let search_matches = Rc::new(RefCell::new(Vec::new()));
        let current_match_idx = Rc::new(RefCell::new(0));

        // Connect node selection to update breadcrumb
        let breadcrumb_clone = lbl_breadcrumb.clone();
        let access_clone = current_access_path.clone();
        let val_clone = current_node_value.clone();
        selection_model.connect_selected_item_notify(move |sel| {
            if let Some(tree_row) = sel.selected_item().and_downcast::<gtk4::TreeListRow>() {
                if let Some(node) = tree_row.item().and_downcast::<JsonNodeObject>() {
                    let p = node.path();
                    let a = node.access_path();
                    let v = node.json_value();
                    breadcrumb_clone.set_text(&p);
                    *access_clone.borrow_mut() = a;
                    *val_clone.borrow_mut() = v;
                }
            }
        });

        // Copy access path button
        let access_for_copy = current_access_path.clone();
        let breadcrumb_for_copy = lbl_breadcrumb.clone();
        btn_copy_path.connect_clicked(move |_| {
            let path = access_for_copy.borrow().clone();
            if !path.is_empty() {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&path);
                }
                let old_text = breadcrumb_for_copy.text().to_string();
                breadcrumb_for_copy.set_text(&format!("{}: {}", tr!("✓ Ruta copiada", "✓ Path copied"), path));
                let lbl = breadcrumb_for_copy.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    lbl.set_text(&old_text);
                });
            }
        });

        // Copy node value button
        let val_for_copy = current_node_value.clone();
        let breadcrumb_for_val = lbl_breadcrumb.clone();
        btn_copy_value.connect_clicked(move |_| {
            let val = val_for_copy.borrow().clone();
            if !val.is_empty() {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&val);
                }
                let old_text = breadcrumb_for_val.text().to_string();
                breadcrumb_for_val.set_text(tr!("✓ Valor copiado al portapapeles", "✓ Value copied to clipboard"));
                let lbl = breadcrumb_for_val.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    lbl.set_text(&old_text);
                });
            }
        });

        // Expand all
        let model_expand = tree_list_model.clone();
        let lv_expand = list_view.clone();
        btn_expand_all.connect_clicked(move |_| {
            let mut i = 0;
            while i < model_expand.n_items() {
                if let Some(row) = model_expand.row(i) {
                    if row.is_expandable() && !row.is_expanded() {
                        row.set_expanded(true);
                    }
                }
                i += 1;
            }
            lv_expand.queue_draw();
        });

        // Collapse all
        let model_collapse = tree_list_model.clone();
        let lv_collapse = list_view.clone();
        btn_collapse_all.connect_clicked(move |_| {
            let mut i = 0;
            while i < model_collapse.n_items() {
                if let Some(row) = model_collapse.row(i) {
                    if row.is_expandable() && row.is_expanded() {
                        row.set_expanded(false);
                    }
                }
                i += 1;
            }
            lv_collapse.queue_draw();
        });

        // Search in Tree
        let model_search = tree_list_model.clone();
        let sel_search = selection_model.clone();
        let matches_search = search_matches.clone();
        let match_idx_search = current_match_idx.clone();
        let count_search = lbl_search_count.clone();

        let run_search = move |query: String| {
            let q = query.trim().to_lowercase();
            matches_search.borrow_mut().clear();
            *match_idx_search.borrow_mut() = 0;

            if q.is_empty() {
                count_search.set_label("");
                return;
            }

            // Expand all rows to search comprehensively
            let mut i = 0;
            while i < model_search.n_items() {
                if let Some(row) = model_search.row(i) {
                    if row.is_expandable() && !row.is_expanded() {
                        row.set_expanded(true);
                    }
                }
                i += 1;
            }

            let mut found_positions = Vec::new();
            for pos in 0..model_search.n_items() {
                if let Some(row) = model_search.row(pos) {
                    if let Some(node) = row.item().and_downcast::<JsonNodeObject>() {
                        let k = node.key().to_lowercase();
                        let v = node.raw_value().to_lowercase();
                        if k.contains(&q) || v.contains(&q) {
                            found_positions.push(pos);
                        }
                    }
                }
            }

            let total = found_positions.len();
            *matches_search.borrow_mut() = found_positions;

            if total > 0 {
                count_search.set_label(&format!("1/{}", total));
                let first_pos = matches_search.borrow()[0];
                sel_search.select_item(first_pos, true);
            } else {
                count_search.set_label(tr!("0 coincidencias", "0 matches"));
            }
        };

        let run_search_clone = Rc::new(run_search);
        let rs1 = run_search_clone.clone();
        search_entry.connect_search_changed(move |entry| {
            rs1(entry.text().to_string());
        });

        // Next / Prev match
        let sel_nav = selection_model.clone();
        let matches_nav = search_matches.clone();
        let match_idx_nav = current_match_idx.clone();
        let count_nav = lbl_search_count.clone();
        btn_next_match.connect_clicked(move |_| {
            let matches = matches_nav.borrow();
            if matches.is_empty() {
                return;
            }
            let mut idx = match_idx_nav.borrow_mut();
            *idx = (*idx + 1) % matches.len();
            count_nav.set_label(&format!("{}/{}", *idx + 1, matches.len()));
            sel_nav.select_item(matches[*idx], true);
        });

        let sel_prev = selection_model.clone();
        let matches_prev = search_matches.clone();
        let match_idx_prev = current_match_idx.clone();
        let count_prev = lbl_search_count.clone();
        btn_prev_match.connect_clicked(move |_| {
            let matches = matches_prev.borrow();
            if matches.is_empty() {
                return;
            }
            let mut idx = match_idx_prev.borrow_mut();
            if *idx == 0 {
                *idx = matches.len() - 1;
            } else {
                *idx -= 1;
            }
            count_prev.set_label(&format!("{}/{}", *idx + 1, matches.len()));
            sel_prev.select_item(matches[*idx], true);
        });

        Self {
            container,
            list_view,
            tree_list_model,
            selection_model,
            lbl_breadcrumb,
            search_entry,
            lbl_search_count,
            btn_expand_all,
            btn_collapse_all,
            btn_copy_path,
            btn_copy_value,
            btn_prev_match,
            btn_next_match,
            current_access_path,
            current_node_value,
            search_matches,
            current_match_idx,
        }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.container
    }

    pub fn refresh_theme(&self, is_dark: bool) {
        let mut child_opt = self.list_view.first_child();
        while let Some(row_widget) = child_opt {
            if let Some(expander) = row_widget.first_child().and_downcast::<gtk4::TreeExpander>() {
                if let (Some(tree_row), Some(label)) = (
                    expander.list_row(),
                    expander.child().and_downcast::<gtk4::Label>(),
                ) {
                    if let Some(node) = tree_row.item().and_downcast::<JsonNodeObject>() {
                        label.set_markup(&node.format_markup(is_dark));
                    }
                }
            }
            child_opt = row_widget.next_sibling();
        }
    }

    pub fn update_language(&self) {
        self.btn_expand_all.set_tooltip_text(Some(tr!(
            "Desplegar y expandir todos los nodos",
            "Expand all nodes"
        )));
        self.btn_collapse_all.set_tooltip_text(Some(tr!(
            "Colapsar y contraer todos los nodos",
            "Collapse all nodes"
        )));
        self.btn_copy_path.set_tooltip_text(Some(tr!(
            "Copiar ruta de acceso (ej. array[0].HotelCode)",
            "Copy access path (e.g. array[0].HotelCode)"
        )));
        self.btn_copy_value.set_tooltip_text(Some(tr!(
            "Copiar valor o sub-JSON del nodo",
            "Copy node value or sub-JSON"
        )));
        self.search_entry.set_placeholder_text(Some(tr!(
            "Buscar nodos o valores...",
            "Search nodes or values..."
        )));
        self.btn_prev_match.set_tooltip_text(Some(tr!(
            "Coincidencia anterior",
            "Previous match"
        )));
        self.btn_next_match.set_tooltip_text(Some(tr!(
            "Siguiente coincidencia",
            "Next match"
        )));
        if self.current_access_path.borrow().is_empty() {
            self.lbl_breadcrumb.set_text(tr!(
                "Selecciona un nodo para ver su ruta...",
                "Select a node to view its path..."
            ));
        }
    }

    pub fn load_json(&self, value: &serde_json::Value) {
        let root_model = self
            .tree_list_model
            .model()
            .downcast::<gio::ListStore>()
            .unwrap();
        root_model.remove_all();

        match value {
            serde_json::Value::Array(_arr) => {
                let root_node = JsonNodeObject::new("array", value, "array", "");
                root_model.append(&root_node);
            }
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let access = format_child_access_path("", k);
                    let node = JsonNodeObject::new(k, v, k, &access);
                    root_model.append(&node);
                }
            }
            _ => {
                let node = JsonNodeObject::new("root", value, "root", "root");
                root_model.append(&node);
            }
        }

        // Expand the first level by default for pleasant user experience
        if let Some(first_row) = self.tree_list_model.row(0) {
            if first_row.is_expandable() {
                first_row.set_expanded(true);
            }
        }

        self.lbl_breadcrumb.set_text(tr!(
            "Selecciona un nodo para ver su ruta...",
            "Select a node to view its path..."
        ));
        *self.current_access_path.borrow_mut() = String::new();
        *self.current_node_value.borrow_mut() = String::new();
        self.lbl_search_count.set_label("");
    }
}
