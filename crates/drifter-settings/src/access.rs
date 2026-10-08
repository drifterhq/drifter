use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
use drifter_core::lang::t;
use drifter_core::settings::{Control, Hit, Id, Look, Mark, Model, Section, Spot, Value};
use drifter_core::{Rect, Vec2};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

pub enum Heard {
    Wake,
    Act(ActionRequest),
}

pub enum Aim {
    Section(Section),
    Spot(Spot),
    Hit(Spot, Hit),
    Slide(Spot, Id, f64, f64, f64),
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
    aims: HashMap<NodeId, Aim>,
}

fn node_id(key: &str) -> NodeId {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    NodeId(hash.finish().max(2))
}

const ROOT: NodeId = NodeId(1);

fn bounds(rect: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: rect.origin.x,
        y0: rect.origin.y,
        x1: rect.max().x,
        y1: rect.max().y,
    }
}

fn mark_name(mark: Mark) -> String {
    match mark {
        Mark::Text(text) => text.to_string(),
        Mark::Plus => t("Increase").to_string(),
        Mark::Minus => t("Decrease").to_string(),
        Mark::Up => t("Move up").to_string(),
        Mark::Down => t("Move down").to_string(),
    }
}

pub struct View<'a> {
    pub model: &'a Model,
    pub look: &'a Look,
    pub section: Section,
    pub spot: Option<Spot>,
    pub scroll: f64,
    pub size: Vec2,
}

fn build(view: &View) -> (TreeUpdate, HashMap<NodeId, Aim>) {
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let mut aims = HashMap::new();
    let mut focus = ROOT;

    let shown = view.model.shown();
    let mut tabs = Vec::new();
    for (at, section) in shown.iter().enumerate() {
        let id = node_id(&format!("section {section:?}"));
        let mut node = Node::new(Role::Tab);
        node.set_label(section.name());
        node.set_selected(*section == view.section);
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
        node.set_bounds(bounds(view.look.item_rect_in(&shown, at)));
        if *section == view.section && view.spot == Some(Spot::Sidebar) {
            focus = id;
        }
        aims.insert(id, Aim::Section(*section));
        tabs.push(id);
        nodes.push((id, node));
    }
    let side = node_id("sidebar");
    let mut list = Node::new(Role::TabList);
    list.set_label(t("Settings"));
    list.set_children(tabs);
    nodes.push((side, list));

    let groups = view.model.groups(view.section);
    let placed = view.look.place(view.size, view.section, &groups, view.scroll);
    let mut parts = Vec::new();
    let heading = node_id("heading");
    let mut title = Node::new(Role::Heading);
    title.set_level(1);
    title.set_label(view.section.name());
    nodes.push((heading, title));
    parts.push(heading);

    for (at, (group, spots)) in groups.iter().zip(&placed).enumerate() {
        let mut rows = Vec::new();
        for (row, rect) in group.rows.iter().zip(&spots.rows) {
            let key = format!("row {:?}", row.id);
            let id = node_id(&key);
            let spot = Spot::Row(row.id, 0);
            let focused = matches!(view.spot, Some(Spot::Row(held, _)) if held == row.id);
            let mut node = match &row.control {
                Control::Switch(on) => {
                    let mut node = Node::new(Role::Switch);
                    node.set_toggled(if *on { Toggled::True } else { Toggled::False });
                    node.add_action(Action::Click);
                    aims.insert(id, Aim::Hit(spot, Hit::Change(row.id, Value::Switch(!on))));
                    node
                }
                Control::Slider { least, most, value, unit, zero } => {
                    let mut node = Node::new(Role::Slider);
                    node.set_numeric_value(*value);
                    node.set_min_numeric_value(*least);
                    node.set_max_numeric_value(*most);
                    let words = match zero {
                        Some(word) if *value == 0.0 => word.to_string(),
                        _ => format!("{}{}", value.round(), unit),
                    };
                    node.set_value(words);
                    node.add_action(Action::Increment);
                    node.add_action(Action::Decrement);
                    node.add_action(Action::SetValue);
                    let step = view
                        .model
                        .nudge(view.section, row.id, 1.0)
                        .and_then(|next| match next {
                            Value::Number(next) => Some((next - value).abs().max(1.0)),
                            _ => None,
                        })
                        .unwrap_or(1.0);
                    aims.insert(id, Aim::Slide(spot, row.id, *least, *most, step));
                    node
                }
                Control::Segments { options, chosen } => {
                    let mut node = Node::new(Role::RadioGroup);
                    let mut kids = Vec::new();
                    for (slot, option) in options.iter().enumerate() {
                        let kid = node_id(&format!("{key} option {slot}"));
                        let mut radio = Node::new(Role::RadioButton);
                        radio.set_label(*option);
                        radio.set_toggled(if slot == *chosen { Toggled::True } else { Toggled::False });
                        radio.add_action(Action::Click);
                        radio.set_bounds(bounds(view.look.segment_rect(*rect, options.len(), slot)));
                        aims.insert(kid, Aim::Hit(spot, Hit::Change(row.id, Value::Choice(slot))));
                        if focused && slot == *chosen {
                            focus = kid;
                        }
                        nodes.push((kid, radio));
                        kids.push(kid);
                    }
                    node.set_children(kids);
                    node
                }
                Control::Buttons(marks) if !marks.is_empty() => {
                    let mut node = Node::new(Role::Group);
                    let mut kids = Vec::new();
                    let rects = view.look.mark_rects(*rect, marks);
                    for (slot, mark) in marks.iter().enumerate() {
                        let kid = node_id(&format!("{key} button {slot}"));
                        let mut button = Node::new(Role::Button);
                        let name = match (mark, marks.contains(&Mark::Plus)) {
                            (Mark::Minus, false) => t("Remove").to_string(),
                            _ => mark_name(*mark),
                        };
                        button.set_label(format!("{}, {}", name, row.label));
                        button.add_action(Action::Click);
                        button.add_action(Action::Focus);
                        button.set_bounds(bounds(rects[slot]));
                        aims.insert(kid, Aim::Hit(Spot::Row(row.id, slot), Hit::Press(row.id, slot)));
                        if matches!(view.spot, Some(Spot::Row(held, part)) if held == row.id && part == slot) {
                            focus = kid;
                        }
                        nodes.push((kid, button));
                        kids.push(kid);
                    }
                    node.set_children(kids);
                    node
                }
                Control::Swatches { colors, chosen } => {
                    let mut node = Node::new(Role::RadioGroup);
                    let mut kids = Vec::new();
                    for slot in 0..colors.len() {
                        let kid = node_id(&format!("{key} swatch {slot}"));
                        let mut radio = Node::new(Role::RadioButton);
                        radio.set_label(drifter_core::lang::tf("Color {}", &[&(slot + 1)]));
                        radio.set_toggled(if Some(slot) == *chosen { Toggled::True } else { Toggled::False });
                        radio.add_action(Action::Click);
                        aims.insert(kid, Aim::Hit(Spot::Row(row.id, slot), Hit::Change(row.id, Value::Choice(slot))));
                        nodes.push((kid, radio));
                        kids.push(kid);
                    }
                    node.set_children(kids);
                    node
                }
                Control::Note(text) => {
                    let mut node = Node::new(Role::Label);
                    node.set_value(text.clone());
                    node
                }
                Control::Buttons(_) if row.opens => {
                    let mut node = Node::new(Role::Button);
                    node.add_action(Action::Click);
                    aims.insert(id, Aim::Hit(spot, Hit::Open(row.id)));
                    node
                }
                Control::Buttons(_) => Node::new(Role::Label),
            };
            node.set_label(row.label.clone());
            if let Some(detail) = &row.detail {
                node.set_description(detail.clone());
            }
            if !row.enabled {
                node.set_disabled();
            }
            node.add_action(Action::Focus);
            node.set_bounds(bounds(*rect));
            if focused && focus == ROOT {
                focus = id;
            }
            aims.entry(id).or_insert(Aim::Spot(spot));
            nodes.push((id, node));
            rows.push(id);
        }
        let gid = node_id(&format!("group {at}"));
        let mut node = Node::new(Role::Group);
        if let Some(header) = group.header {
            node.set_label(header);
        }
        if let Some(footer) = &group.footer {
            node.set_description(footer.clone());
        }
        node.set_children(rows);
        nodes.push((gid, node));
        parts.push(gid);
    }
    let page = node_id("page");
    let mut panel = Node::new(Role::TabPanel);
    panel.set_label(view.section.name());
    panel.set_children(parts);
    nodes.push((page, panel));

    let mut root = Node::new(Role::Window);
    root.set_label(t("Settings"));
    root.set_children(vec![side, page]);
    nodes.push((ROOT, root));

    let mut info = TreeInfo::new(ROOT);
    info.toolkit_name = Some("Drifter".to_string());
    let update = TreeUpdate {
        nodes,
        tree: Some(info),
        tree_id: TreeId::ROOT,
        focus,
    };
    (update, aims)
}

impl Access {
    pub fn start(heard: Sender<Heard>) -> Access {
        let adapter = accesskit_unix::Adapter::new(Wake(heard.clone()), Acts(heard), Rest);
        Access {
            adapter,
            aims: HashMap::new(),
        }
    }

    pub fn update(&mut self, view: &View) {
        let mut built = None;
        self.adapter.update_if_active(|| {
            let (update, aims) = build(view);
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
        let aim = self.aims.get(&request.target_node)?;
        Some(match (aim, request.action) {
            (Aim::Section(section), _) => Order::Section(*section),
            (Aim::Spot(spot), _) => Order::Focus(*spot),
            (Aim::Hit(spot, hit), Action::Click) => Order::Act(*spot, *hit),
            (Aim::Hit(spot, _), _) => Order::Focus(*spot),
            (Aim::Slide(spot, id, least, most, step), action) => match (action, request.data.as_ref()) {
                (Action::SetValue, Some(ActionData::NumericValue(value))) => {
                    Order::Act(*spot, Hit::Change(*id, Value::Number(value.clamp(*least, *most))))
                }
                (Action::Increment, _) => Order::Nudge(*spot, *id, *step),
                (Action::Decrement, _) => Order::Nudge(*spot, *id, -*step),
                _ => Order::Focus(*spot),
            },
        })
    }
}

pub enum Order {
    Section(Section),
    Focus(Spot),
    Act(Spot, Hit),
    Nudge(Spot, Id, f64),
}
