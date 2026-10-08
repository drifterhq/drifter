use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use accesskit::{Action, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use drifter_core::activity;
use drifter_core::activity::look::{Hit, Model, View};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

pub enum Heard {
    Wake,
    Act(ActionRequest),
}

pub enum Order {
    Hit(Hit),
    Force,
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
    aims: HashMap<NodeId, Order>,
    said: Option<(String, String)>,
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

fn build(model: &Model, said: &mut Option<(String, String)>) -> (TreeUpdate, HashMap<NodeId, Order>) {
    let mut nodes = Vec::new();
    let mut aims = HashMap::new();
    let mut focus = ROOT;

    let mut tabs = Vec::new();
    for view in View::ALL {
        let id = node_id(&format!("view {view:?}"));
        let mut node = clickable(Role::Tab, view.name());
        node.set_selected(view == model.view);
        aims.insert(id, Order::Hit(Hit::View(view)));
        nodes.push((id, node));
        tabs.push(id);
    }
    let bar = node_id("views");
    let mut list = Node::new(Role::TabList);
    list.set_children(tabs);
    nodes.push((bar, list));

    let mut rows = Vec::new();
    for group in model.shown() {
        let id = node_id(&format!("group {}", group.key));
        let impact = group.impact(model.memory.total_kb).word();
        let (cpu, memory) = (activity::percent_words(group.cpu), activity::memory_words(group.mem_kb));
        let mut words = drifter_core::lang::tf("{}, CPU {}, memory {}", &[&impact, &cpu, &memory]);
        let picked = model.selected.as_deref() == Some(group.key.as_str());
        if picked {
            match said.as_ref() {
                Some((key, held)) if *key == group.key => words = held.clone(),
                _ => *said = Some((group.key.clone(), words.clone())),
            }
        }
        let mut node = clickable(Role::ListItem, &group.name);
        node.set_description(words);
        node.set_selected(picked);
        if picked {
            focus = id;
        }
        aims.insert(id, Order::Hit(Hit::Row(group.key.clone())));
        nodes.push((id, node));
        rows.push(id);
    }
    let table = node_id("table");
    let mut list = Node::new(Role::List);
    list.set_label(model.view.name());
    list.set_children(rows);
    nodes.push((table, list));

    let mut top = vec![bar, table];
    if let Some(group) = model.chosen() {
        let mut kids = Vec::new();
        let lines = [
            (drifter_core::lang::t("Doing"), group.detail.clone()),
            ("CPU", percent_or_paused(group)),
            (drifter_core::lang::t("Memory"), activity::memory_words(group.mem_kb)),
            (drifter_core::lang::t("Processes"), group.members.len().to_string()),
        ];
        for (at, (name, value)) in lines.iter().enumerate() {
            if value.is_empty() {
                continue;
            }
            let id = node_id(&format!("detail {at}"));
            let mut node = Node::new(Role::Label);
            node.set_label(format!("{name}: {value}"));
            nodes.push((id, node));
            kids.push(id);
        }
        if group.can_stop() {
            let pause = if group.paused { drifter_core::lang::t("Resume") } else { drifter_core::lang::t("Pause") };
            for (key, label, order) in [
                ("pause", pause, Order::Hit(Hit::Pause)),
                ("quit", drifter_core::lang::t("Quit"), Order::Hit(Hit::Quit)),
                ("force", drifter_core::lang::t("Force quit"), Order::Force),
            ] {
                let id = node_id(&format!("button {key}"));
                nodes.push((id, clickable(Role::Button, label)));
                aims.insert(id, order);
                kids.push(id);
            }
        }
        let panel = node_id("detail");
        let mut node = Node::new(Role::Group);
        node.set_label(group.name.clone());
        node.set_children(kids);
        nodes.push((panel, node));
        top.push(panel);
    }
    if let Some(toast) = &model.toast {
        let id = node_id("toast");
        let mut node = Node::new(Role::Label);
        node.set_label(toast.words.clone());
        node.set_live(accesskit::Live::Polite);
        nodes.push((id, node));
        top.push(id);
    }

    let mut root = Node::new(Role::Window);
    root.set_label(drifter_core::lang::t("Activity"));
    root.set_children(top);
    nodes.push((ROOT, root));
    let mut info = TreeInfo::new(ROOT);
    info.toolkit_name = Some("Drifter".to_string());
    (
        TreeUpdate {
            nodes,
            tree: Some(info),
            tree_id: TreeId::ROOT,
            focus,
        },
        aims,
    )
}

fn percent_or_paused(group: &activity::Group) -> String {
    match group.paused {
        true => drifter_core::lang::t("Paused").to_string(),
        false => activity::percent_words(group.cpu),
    }
}

impl Access {
    pub fn start(heard: Sender<Heard>) -> Access {
        Access {
            adapter: accesskit_unix::Adapter::new(Wake(heard.clone()), Acts(heard), Rest),
            aims: HashMap::new(),
            said: None,
        }
    }

    pub fn update(&mut self, model: &Model) {
        let mut built = None;
        let said = &mut self.said;
        self.adapter.update_if_active(|| {
            let (update, aims) = build(model, said);
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
        match (self.aims.get(&request.target_node)?, request.action) {
            (Order::Hit(Hit::Row(key)), Action::Click | Action::Focus) => Some(Order::Hit(Hit::Row(key.clone()))),
            (Order::Hit(Hit::View(view)), Action::Click | Action::Focus) => Some(Order::Hit(Hit::View(*view))),
            (Order::Hit(hit), Action::Click) => Some(Order::Hit(hit.clone())),
            (Order::Force, Action::Click) => Some(Order::Force),
            _ => None,
        }
    }
}
