use super::*;
use accesskit::{Live, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};

pub(super) struct Speech {
    adapter: accesskit_unix::Adapter,
    owns: bool,
}

struct Wake(smithay::reexports::calloop::channel::Sender<()>);

impl accesskit::ActivationHandler for Wake {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self.0.send(());
        None
    }
}

struct Ignore;

impl accesskit::ActionHandler for Ignore {
    fn do_action(&mut self, _: accesskit::ActionRequest) {}
}

impl accesskit::DeactivationHandler for Ignore {
    fn deactivate_accessibility(&mut self) {}
}

const ROOT: NodeId = NodeId(1);

fn id(key: &str) -> NodeId {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hash);
    NodeId(hash.finish().max(2))
}

fn labelled(role: Role, label: &str) -> Node {
    let mut node = Node::new(role);
    node.set_label(label);
    node
}

impl State {
    pub(super) fn start_speech(&mut self) {
        if self.speech.is_some() {
            return;
        }
        let (wake, woken) = smithay::reexports::calloop::channel::channel::<()>();
        let watched = self.handle.insert_source(woken, |event, _, state: &mut State| {
            if let smithay::reexports::calloop::channel::Event::Msg(()) = event {
                state.speak();
            }
        });
        if let Err(error) = watched {
            log!("drifter: the screen reader cannot hear the desktop: {error}");
            return;
        }
        self.speech = Some(Speech {
            adapter: accesskit_unix::Adapter::new(Wake(wake), Ignore, Ignore),
            owns: false,
        });
    }

    pub(super) fn speak(&mut self) {
        let Some(mut speech) = self.speech.take() else {
            return;
        };
        let owns = self.lock.is_some() || self.switch.is_some() || self.search.is_some();
        if owns != speech.owns {
            speech.owns = owns;
            speech.adapter.update_window_focus_state(owns);
        }
        speech.adapter.update_if_active(|| self.shell_tree());
        self.speech = Some(speech);
    }

    fn shell_tree(&self) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut kids = Vec::new();
        let mut focus = ROOT;

        if let Some(lock) = self.lock.as_ref() {
            let field = id("lock password");
            let node = labelled(Role::PasswordInput, drifter_core::lang::t("Password"));
            nodes.push((field, node));
            let mut parts = vec![field];
            if lock.note == drifter_core::lock::Note::Wrong {
                let note = id(&format!("lock wrong {:?}", lock.typed));
                let mut words = labelled(Role::Alert, lock.note.text());
                words.set_live(Live::Assertive);
                nodes.push((note, words));
                parts.push(note);
            }
            let dialog = id("lock");
            let mut node = labelled(Role::Dialog, drifter_core::lang::t("Lock Screen"));
            node.set_children(parts);
            nodes.push((dialog, node));
            kids.push(dialog);
            focus = field;
        } else if let Some(open) = self.switch.as_ref() {
            let mut items = Vec::new();
            for (at, window_id) in open.order.iter().enumerate() {
                let Some(window) = self.windows.iter().find(|window| window.id == *window_id) else {
                    continue;
                };
                let item = id(&format!("switch {window_id}"));
                let name = match (window.title.is_empty(), window.app.is_empty()) {
                    (false, _) => window.title.clone(),
                    (true, false) => window.app.clone(),
                    (true, true) => drifter_core::lang::t("Window").to_string(),
                };
                let mut node = labelled(Role::ListItem, &name);
                node.set_selected(at == open.at);
                if at == open.at {
                    focus = item;
                }
                nodes.push((item, node));
                items.push(item);
            }
            let list = id("switcher");
            let mut node = labelled(Role::List, drifter_core::lang::t("Switch windows"));
            node.set_children(items);
            nodes.push((list, node));
            kids.push(list);
        } else if let Some(open) = self.search.as_ref() {
            let field = id("search field");
            let mut node = labelled(Role::SearchInput, drifter_core::lang::t("Search"));
            node.set_value(open.query.clone());
            nodes.push((field, node));
            focus = field;
            let mut items = Vec::new();
            for (at, found) in open.results.iter().enumerate().take(50) {
                let item = id(&format!("found {at} {}", found.key));
                let mut node = labelled(Role::ListItem, &found.title);
                if !found.detail.is_empty() {
                    node.set_description(found.detail.clone());
                }
                node.set_selected(at == open.selected);
                if at == open.selected {
                    focus = item;
                }
                nodes.push((item, node));
                items.push(item);
            }
            let list = id("search results");
            let mut node = Node::new(Role::List);
            node.set_children(items);
            nodes.push((list, node));
            let dialog = id("search");
            let mut node = labelled(Role::Dialog, drifter_core::lang::t("Search"));
            node.set_children(vec![field, list]);
            nodes.push((dialog, node));
            kids.push(dialog);
        }

        for card in &self.cards {
            let note = &card.note;
            let item = id(&format!("note {}", note.id));
            let words = match note.body.trim().is_empty() {
                true => format!("{}: {}", note.app, note.summary),
                false => format!("{}: {}. {}", note.app, note.summary, note.body),
            };
            let mut node = labelled(Role::Alert, &words);
            node.set_live(if note.critical { Live::Assertive } else { Live::Polite });
            nodes.push((item, node));
            kids.push(item);
        }

        let mut root = labelled(Role::Window, "Drifter");
        root.set_children(kids);
        nodes.push((ROOT, root));
        let mut info = TreeInfo::new(ROOT);
        info.toolkit_name = Some("Drifter".to_string());
        TreeUpdate {
            nodes,
            tree: Some(info),
            tree_id: TreeId::ROOT,
            focus,
        }
    }
}
