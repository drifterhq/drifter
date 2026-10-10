use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use accesskit::{Action, ActionRequest, Live, Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
use drifter_core::explorer::model::{Act, Item, Line, Model, Pane, Sheet, title_of};
use drifter_core::explorer::ops::Clash;
use drifter_core::explorer::prefs::Mode;
use drifter_core::lang::{t, tf};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

pub enum Heard {
    Wake,
    Act(ActionRequest),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Order {
    Act(Act),
    Pick(PathBuf),
    Open(PathBuf),
    Menu(Act),
}

struct Wake(Sender<Heard>);

impl accesskit::ActivationHandler for Wake {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self.0.send(Heard::Wake);
        None
    }
}

struct Acts(Sender<Heard>);

impl accesskit::ActionHandler for Acts {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.0.send(Heard::Act(request));
    }
}

struct Rest;

impl accesskit::DeactivationHandler for Rest {
    fn deactivate_accessibility(&mut self) {}
}

pub struct Access {
    adapter: accesskit_unix::Adapter,
    aims: HashMap<NodeId, (Order, Option<Order>)>,
}

const ROOT: NodeId = NodeId(1);

fn node_id(key: &str) -> NodeId {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    NodeId(hash.finish().max(2))
}

fn clickable(role: Role, label: &str) -> Node {
    let mut node = Node::new(role);
    node.set_label(label);
    node.add_action(Action::Click);
    node.add_action(Action::Focus);
    node
}

#[derive(Default)]
struct Tree {
    nodes: Vec<(NodeId, Node)>,
    aims: HashMap<NodeId, (Order, Option<Order>)>,
}

impl Tree {
    fn put(&mut self, key: &str, node: Node) -> NodeId {
        let id = node_id(key);
        self.nodes.push((id, node));
        id
    }

    fn button(&mut self, key: &str, label: &str, act: Act, on: bool) -> NodeId {
        let mut node = clickable(Role::Button, label);
        if !on {
            node.set_disabled();
        }
        let id = self.put(key, node);
        if on {
            self.aims.insert(id, (Order::Act(act), None));
        }
        id
    }

    fn group(&mut self, key: &str, role: Role, label: &str, kids: Vec<NodeId>) -> NodeId {
        let mut node = Node::new(role);
        if !label.is_empty() {
            node.set_label(label);
        }
        node.set_children(kids);
        self.put(key, node)
    }

    fn label(&mut self, key: &str, words: &str) -> NodeId {
        let mut node = Node::new(Role::Label);
        node.set_label(words);
        node.set_value(words);
        self.put(key, node)
    }

    fn menu(&mut self, key: &str, items: &[Item], hover: Option<usize>, focus: &mut Option<NodeId>) -> Vec<NodeId> {
        let mut kids = Vec::new();
        for (at, item) in items.iter().enumerate() {
            if item.rule {
                continue;
            }
            let role = match item.check {
                Some(_) => Role::MenuItemCheckBox,
                None => Role::MenuItem,
            };
            let mut node = clickable(role, &item.label);
            if let Some(on) = item.check {
                node.set_toggled(if on { Toggled::True } else { Toggled::False });
            }
            if !item.on {
                node.set_disabled();
            }
            if !item.keys.is_empty() {
                node.set_keyboard_shortcut(item.keys);
            }
            let item_key = format!("{key} {at}");
            if !item.kids.is_empty() {
                let inner = self.menu(&item_key, &item.kids, None, &mut None);
                node.set_children(inner);
            }
            let id = self.put(&item_key, node);
            if let (true, Some(act)) = (item.on, item.act.clone()) {
                self.aims.insert(id, (Order::Menu(act), None));
            }
            if hover == Some(at) || (hover.is_none() && focus.is_none() && item.on) {
                *focus = Some(id);
            }
            kids.push(id);
        }
        kids
    }
}

fn build(model: &mut Model) -> (TreeUpdate, HashMap<NodeId, (Order, Option<Order>)>) {
    let lines = model.lines();
    let mut tree = Tree::default();
    let mut focus = None;
    let mut top = Vec::new();

    let mut tools = vec![
        tree.button("back", t("Back"), Act::Back, !model.back.is_empty()),
        tree.button("forward", t("Forward"), Act::Forward, !model.ahead.is_empty()),
        tree.button("up", t("Enclosing Folder"), Act::Up, model.dir.parent().is_some()),
    ];
    let place = model.dir.to_string_lossy().into_owned();
    match &model.path {
        Some(field) => {
            let mut node = Node::new(Role::TextInput);
            node.set_label(t("Path"));
            node.set_value(field.text.clone());
            let id = tree.put("path", node);
            focus = Some(id);
            tools.push(id);
        }
        None => tools.push(tree.button("path", &tf("Path: {}", &[&place]), Act::EditPath, true)),
    }
    match model.search.as_ref() {
        Some(search) => {
            let mut node = Node::new(Role::SearchInput);
            node.set_label(t("Search"));
            node.set_value(search.field.text.clone());
            let id = tree.put("search", node);
            if search.focused {
                focus = Some(id);
            }
            tools.push(id);
        }
        None => tools.push(tree.button("search", t("Search"), Act::Search, true)),
    }
    let mut modes = Vec::new();
    for mode in Mode::ALL {
        let mut node = clickable(Role::RadioButton, t(mode.name()));
        node.set_toggled(if model.view.mode == mode { Toggled::True } else { Toggled::False });
        let id = tree.put(&format!("mode {mode:?}"), node);
        tree.aims.insert(id, (Order::Act(Act::Show(mode)), None));
        modes.push(id);
    }
    tools.push(tree.group("modes", Role::RadioGroup, t("View"), modes));
    tools.push(tree.button("settings", t("Settings"), Act::Prefs, true));
    top.push(tree.group("tools", Role::Toolbar, "", tools));

    if model.tabs.len() > 1 {
        let mut tabs = Vec::new();
        for (at, tab) in model.tabs.iter().enumerate() {
            let mut node = clickable(Role::Tab, &title_of(&tab.dir));
            node.set_selected(at == model.tab);
            let id = tree.put(&format!("tab {at}"), node);
            tree.aims.insert(id, (Order::Act(Act::ShowTab(at)), None));
            tabs.push(id);
        }
        top.push(tree.group("tabs", Role::TabList, t("Tabs"), tabs));
    }

    let mut side = Vec::new();
    let mut heads = 0;
    for (at, line) in lines.iter().enumerate() {
        let key = format!("line {at}");
        let id = match line {
            Line::Head(words) => {
                heads += 1;
                let mut node = Node::new(Role::Heading);
                node.set_label(t(words));
                tree.put(&format!("head {heads}"), node)
            }
            Line::Place { path, name, .. } => line_node(&mut tree, &key, name, path, None),
            Line::Folder { path, depth, open } => {
                let name = drifter_core::explorer::look::tree_name(model, path);
                line_node(&mut tree, &key, &name, path, Some((*depth, *open)))
            }
            Line::Drive { path, name, drive } => match (path, drive.as_ref().and_then(|drive| drive.device.clone())) {
                (Some(path), _) => line_node(&mut tree, &key, name, path, None),
                (None, Some(device)) => {
                    let mut node = clickable(Role::TreeItem, name);
                    node.set_description(t("Not mounted"));
                    let id = tree.put(&key, node);
                    tree.aims.insert(id, (Order::Act(Act::Mount(device)), None));
                    id
                }
                (None, None) => tree.label(&key, name),
            },
        };
        if model.pane == Pane::Side && model.side_at == Some(at) {
            focus.get_or_insert(id);
        }
        side.push(id);
    }
    top.push(tree.group("side", Role::Tree, t("Sidebar"), side));

    let mut rows = Vec::new();
    let renaming = model.renaming.as_ref().map(|(path, field)| (path.clone(), field.text.clone()));
    let shown = near(model.focus_index().unwrap_or(0), model.entries.len());
    for (at, entry) in model.entries.iter().enumerate().skip(shown.start).take(shown.len()) {
        let key = format!("file {}", entry.path.display());
        let picked = model.picked.contains(&entry.path);
        let id = match &renaming {
            Some((path, text)) if *path == entry.path => {
                let mut node = Node::new(Role::TextInput);
                node.set_label(t("Rename"));
                node.set_description(entry.name.clone());
                node.set_value(text.clone());
                let id = tree.put(&key, node);
                focus = Some(id);
                id
            }
            _ => {
                let mut node = clickable(Role::ListItem, &entry.name);
                let size = match entry.folder {
                    true => String::new(),
                    false => format!(", {}", drifter_core::explorer::entry::size_words(entry.size)),
                };
                node.set_description(format!("{}{size}, {}", entry.kind_words(), model.date(entry.modified)));
                node.set_selected(picked);
                node.set_position_in_set(at);
                let id = tree.put(&key, node);
                tree.aims.insert(id, (Order::Open(entry.path.clone()), Some(Order::Pick(entry.path.clone()))));
                id
            }
        };
        if model.pane == Pane::Files && model.focus.as_ref() == Some(&entry.path) {
            focus.get_or_insert(id);
        }
        rows.push(id);
    }
    let mut list = Node::new(Role::ListBox);
    list.set_label(title_of(&model.dir));
    list.set_children(rows);
    list.set_size_of_set(model.entries.len());
    if let Some(trouble) = &model.trouble {
        list.set_description(trouble.clone());
    }
    let files = tree.put("files", list);
    top.push(files);

    top.push(tree.label("status", &model.summary()));
    let said = model.alerts.last().cloned().or_else(|| model.note.as_ref().map(|(words, _)| words.clone()));
    if let Some(words) = said {
        let mut node = Node::new(Role::Label);
        node.set_label(words.clone());
        node.set_value(words);
        node.set_live(Live::Polite);
        top.push(tree.put("said", node));
    }

    if let Some(sheet) = model.sheet.clone() {
        let id = sheet_node(&mut tree, &sheet, &mut focus);
        top.push(id);
    }
    if let Some(menu) = model.menu.clone() {
        let mut inner = None;
        let kids = tree.menu("menu", &menu.items, menu.hover, &mut inner);
        top.push(tree.group("menu", Role::Menu, "", kids));
        if inner.is_some() {
            focus = inner;
        }
    }

    let mut root = Node::new(Role::Window);
    root.set_label(title_of(&model.dir));
    root.set_children(top);
    tree.nodes.push((ROOT, root));
    let mut info = TreeInfo::new(ROOT);
    info.toolkit_name = Some("Drifter".to_string());
    let focus = focus.unwrap_or(files);
    (TreeUpdate { nodes: tree.nodes, tree: Some(info), tree_id: TreeId::ROOT, focus }, tree.aims)
}

const MOST_ROWS: usize = 200;

fn near(focus: usize, count: usize) -> std::ops::Range<usize> {
    let end = (focus.saturating_sub(MOST_ROWS / 2) + MOST_ROWS).min(count);
    end.saturating_sub(MOST_ROWS)..end
}

fn line_node(tree: &mut Tree, key: &str, name: &str, path: &std::path::Path, folder: Option<(usize, bool)>) -> NodeId {
    let mut node = clickable(Role::TreeItem, name);
    if let Some((depth, open)) = folder {
        node.set_level(depth + 1);
        node.set_expanded(open);
    }
    let id = tree.put(key, node);
    tree.aims.insert(id, (Order::Act(Act::Go(path.to_path_buf())), None));
    id
}

fn sheet_node(tree: &mut Tree, sheet: &Sheet, focus: &mut Option<NodeId>) -> NodeId {
    let mut kids = Vec::new();
    let title = match sheet {
        Sheet::Prefs(_) => t("Settings").to_string(),
        Sheet::Info(info) => info.entry.name.clone(),
        Sheet::Confirm { title, .. } => title.clone(),
        Sheet::Clash { names, into, .. } => match names.as_slice() {
            [one] => tf("“{}” is already in {}", &[one, &title_of(into)]),
            _ => tf("{} items are already in {}", &[&names.len(), &title_of(into)]),
        },
        Sheet::Keys => t("Keyboard Shortcuts").to_string(),
        Sheet::About => t("About Explorer").to_string(),
        Sheet::Apps(chooser) => tf("Open “{}” with", &[&chooser.name]),
        Sheet::Look => t("Quick Look").to_string(),
    };
    match sheet {
        Sheet::Confirm { words, yes, act, .. } => {
            kids.push(tree.label("sheet words", words));
            kids.push(tree.button("sheet no", t("Cancel"), Act::CloseSheet, true));
            kids.push(tree.button("sheet yes", yes, act.clone(), true));
        }
        Sheet::Clash { names, .. } => {
            kids.push(tree.label("sheet words", &names.join(", ")));
            kids.push(tree.button("sheet no", t("Cancel"), Act::CloseSheet, true));
            kids.push(tree.button("sheet skip", t("Skip"), Act::Clash(Clash::Skip), true));
            kids.push(tree.button("sheet replace", t("Replace"), Act::Clash(Clash::Replace), true));
            kids.push(tree.button("sheet both", t("Keep Both"), Act::Clash(Clash::KeepBoth), true));
        }
        Sheet::Info(info) => {
            let entry = &info.entry;
            match &info.naming {
                Some(field) => {
                    let mut node = Node::new(Role::TextInput);
                    node.set_label(t("Rename"));
                    node.set_value(field.text.clone());
                    let id = tree.put("sheet name", node);
                    *focus = Some(id);
                    kids.push(id);
                }
                None => kids.push(tree.button("sheet rename", t("Rename"), Act::NameInInfo, true)),
            }
            kids.push(tree.label("sheet kind", &entry.kind_words()));
            if !entry.folder {
                kids.push(tree.label("sheet size", &drifter_core::explorer::entry::size_words(entry.size)));
            }
            kids.push(tree.label("sheet where", &entry.path.to_string_lossy()));
            if let Some((bytes, count)) = info.total {
                let words = tf("{} in {}", &[&drifter_core::explorer::entry::size_words(bytes), &drifter_core::explorer::entry::items(count as usize)]);
                kids.push(tree.label("sheet total", &words));
            }
            if let Some(app) = &info.opens {
                kids.push(tree.label("sheet opens", &format!("{}: {app}", t("Opens with"))));
                kids.push(tree.button("sheet change", t("Change…"), Act::ChangeApp, true));
            }
            for (key, label, on, act) in [("sheet lock", t("Locked"), info.locked(), Act::Lock), ("sheet runs", t("Can run as a program"), info.runs(), Act::Runs)] {
                let Some(on) = on else {
                    continue;
                };
                let mut node = clickable(Role::CheckBox, label);
                node.set_toggled(if on { Toggled::True } else { Toggled::False });
                let id = tree.put(key, node);
                tree.aims.insert(id, (Order::Act(act), None));
                kids.push(id);
            }
            kids.push(tree.button("sheet close", t("Done"), Act::CloseSheet, true));
        }
        Sheet::Apps(chooser) => {
            let mut rows = Vec::new();
            for (at, app) in chooser.apps.iter().enumerate() {
                let mut node = clickable(Role::ListItem, &app.name);
                node.set_selected(chooser.at == Some(at));
                let id = tree.put(&format!("app {}", app.id), node);
                tree.aims.insert(id, (Order::Act(Act::PickApp(at)), Some(Order::Act(Act::PickApp(at)))));
                if chooser.at == Some(at) {
                    *focus = Some(id);
                }
                rows.push(id);
            }
            kids.push(tree.group("sheet apps", Role::ListBox, t("Apps"), rows));
            let mut always = clickable(Role::CheckBox, &tf("Always use this app for {} files", &[&chooser.kind]));
            always.set_toggled(if chooser.always { Toggled::True } else { Toggled::False });
            let id = tree.put("sheet always", always);
            tree.aims.insert(id, (Order::Act(Act::AlwaysApp), None));
            kids.push(id);
            kids.push(tree.button("sheet no", t("Cancel"), Act::CloseSheet, true));
            kids.push(tree.button("sheet open", t("Open"), Act::ChooseApp, chooser.at.is_some()));
            return tree.group("sheet", Role::Dialog, &title, kids);
        }
        _ => kids.push(tree.button("sheet close", t("Done"), Act::CloseSheet, true)),
    }
    if let Some(&last) = kids.last() {
        *focus = Some(last);
    }
    tree.group("sheet", Role::Dialog, &title, kids)
}

impl Access {
    pub fn start(heard: Sender<Heard>) -> Access {
        Access { adapter: accesskit_unix::Adapter::new(Wake(heard.clone()), Acts(heard), Rest), aims: HashMap::new() }
    }

    pub fn update(&mut self, model: &mut Model) {
        let mut built = None;
        self.adapter.update_if_active(|| {
            let (update, aims) = build(model);
            built = Some(aims);
            update
        });
        if let Some(aims) = built {
            self.aims = aims;
        }
    }

    pub fn focused(&mut self, on: bool) {
        self.adapter.update_window_focus_state(on);
    }

    pub fn aim(&self, request: &ActionRequest) -> Option<Order> {
        let (click, focus) = self.aims.get(&request.target_node)?;
        match request.action {
            Action::Click => Some(click.clone()),
            Action::Focus => focus.clone(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drifter-explorer-access-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files/Pictures")).unwrap();
        std::fs::write(dir.join("files/note.txt"), "hi").unwrap();
        dir
    }

    fn model(dir: &std::path::Path) -> Model {
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("files")));
        for _ in 0..50 {
            model.settle_all(std::time::Duration::from_millis(100));
            model.lines();
            if !model.entries.is_empty() {
                break;
            }
        }
        model
    }

    fn named<'a>(update: &'a TreeUpdate, label: &str) -> Option<(NodeId, &'a Node)> {
        update.nodes.iter().find(|(_, node)| node.label() == Some(label)).map(|(id, node)| (*id, node))
    }

    #[test]
    fn a_screen_reader_sees_the_files_and_can_open_and_pick_them() {
        let dir = scratch("files");
        let mut model = model(&dir);
        let note = dir.join("files/note.txt");
        model.pick_only(note.clone());
        let (update, aims) = build(&mut model);
        let (id, node) = named(&update, "note.txt").expect("the file is a node");
        assert_eq!(node.role(), Role::ListItem);
        assert!(node.is_selected().unwrap_or(false));
        assert!(node.description().is_some_and(|words| words.contains("2 bytes")), "{:?}", node.description());
        assert_eq!(update.focus, id);
        assert_eq!(aims[&id], (Order::Open(note.clone()), Some(Order::Pick(note))));
        let (folder, _) = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Pictures") && node.role() == Role::ListItem)
            .expect("the folder is a node");
        assert!(matches!(aims[folder].0, Order::Open(_)));
        let (back, node) = named(&update, "Back").expect("Back is a button");
        assert!(node.is_disabled());
        assert!(!aims.contains_key(&back));
        let (_, node) = named(&update, "List").expect("the view modes are there");
        assert_eq!(node.toggled(), Some(Toggled::True));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn big_folders_send_only_the_rows_near_the_focus() {
        assert_eq!(near(0, 20000), 0..200);
        assert_eq!(near(5000, 20000), 4900..5100);
        assert_eq!(near(19999, 20000), 19800..20000);
        assert_eq!(near(3, 7), 0..7);
        let dir = scratch("big");
        for n in 0..500 {
            std::fs::write(dir.join(format!("files/n{n:03}.txt")), "").unwrap();
        }
        let mut model = model(&dir);
        let last = dir.join("files/n499.txt");
        model.pick_only(last.clone());
        let (update, _) = build(&mut model);
        let rows = update.nodes.iter().filter(|(_, node)| node.role() == Role::ListItem).count();
        assert_eq!(rows, MOST_ROWS);
        let (_, node) = named(&update, "n499.txt").expect("the focused file is there");
        assert_eq!(node.position_in_set(), Some(500));
        let (_, list) = update.nodes.iter().find(|(_, node)| node.role() == Role::ListBox).unwrap();
        assert_eq!(list.size_of_set(), Some(502));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn menus_sheets_and_renames_take_the_focus() {
        let dir = scratch("focus");
        let mut model = model(&dir);
        model.pick_only(dir.join("files/note.txt"));
        model.act(Act::Rename);
        let (update, _) = build(&mut model);
        let (id, node) = named(&update, "Rename").expect("the rename box is a node");
        assert_eq!(node.role(), Role::TextInput);
        assert_eq!(node.value(), Some("note.txt"));
        assert_eq!(update.focus, id);
        model.renaming = None;

        model.act(Act::Trash);
        model.act(Act::Keys);
        let (update, aims) = build(&mut model);
        let (close, _) = named(&update, "Done").expect("the sheet has a Done button");
        assert_eq!(update.focus, close);
        assert_eq!(aims[&close].0, Order::Act(Act::CloseSheet));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
