use std::collections::{hash_map::Entry, HashMap};

use parking_lot::RwLock;

use godot::prelude::*;

use super::{
    frame::{Frame, SpawnRecord},
    PlayStageOwner,
};

pub struct SpawnManager {
    spawned_nodes: RwLock<HashMap<String, Gd<Node>>>,
}

impl SpawnManager {
    pub fn new() -> Self {
        Self {
            spawned_nodes: RwLock::new(HashMap::new()),
        }
    }

    pub fn load_frame(&self, owner: &mut impl PlayStageOwner, frame: &Frame) {
        self.remove_despawned_nodes(owner, frame);
        self.spawn_missing_nodes(owner, frame);
    }

    pub fn despawn(&self, owner: &mut impl PlayStageOwner, node_path: &str, frame: &Frame) {
        if let Some(mut node) = owner.get_node(node_path.into()) {
            if node.has_method("networked_despawn".into()) {
                node.call("networked_despawn".into(), &[]);
            }

            if let Some(mut parent) = node.get_parent() {
                parent.remove_child(node.clone());
            }

            node.queue_free();

            self.spawned_nodes.write().remove(node_path);
            frame.remove_spawn_record(&node_path);

            owner.update(|_, cx| {
                cx.logger()
                    .event_for_frame(frame.tick(), "despawned".into(), node_path.to_string(), cx)
                    .unwrap();
            });
        }
    }

    pub fn spawn(
        &self,
        owner: &mut impl PlayStageOwner,
        mut spawn_record: SpawnRecord,
        frame: &Frame,
        resurrecting: bool,
    ) -> Gd<Node> {
        let mut spawned_node = spawn_record.scene.instantiate().unwrap();

        if !resurrecting {
            spawn_record.name = frame.avoid_name_collision(spawn_record.name);
        }
        spawned_node.set_name(spawn_record.name.clone().into());

        if spawned_node.has_method("networked_spawn".into()) {
            spawned_node.call("networked_spawn".into(), &[spawn_record.state.clone()]);
        }

        let mut parent = owner.get_node(&spawn_record.parent_path).unwrap();
        parent.add_child(spawned_node.clone());

        let node_path = spawned_node.get_path().to_string();

        self.spawned_nodes
            .write()
            .insert(node_path.clone(), spawned_node.clone());
        frame.add_spawn_record(node_path.clone(), spawn_record.clone());

        owner.update(|_, cx| {
            cx.logger()
                .event_for_frame(frame.tick(), "spawned".into(), node_path.into(), cx)
                .unwrap();
        });

        spawned_node
    }

    fn remove_despawned_nodes(&self, owner: &mut impl PlayStageOwner, frame: &Frame) {
        let mut nodes_to_despawn = Vec::new();

        for spawned_node_path in self.spawned_nodes.read().keys() {
            if !frame.contains_spawn_record(spawned_node_path) {
                nodes_to_despawn.push(spawned_node_path.clone());
            }
        }

        for spawned_node in nodes_to_despawn {
            self.despawn(owner, &spawned_node, frame);
        }
    }

    fn spawn_missing_nodes(&self, owner: &mut impl PlayStageOwner, frame: &Frame) {
        let mut nodes_to_spawn = Vec::new();

        for node_path in frame.spawned_node_paths() {
            let mut spawned_nodes = self.spawned_nodes.write();
            if let Entry::Occupied(entry) = spawned_nodes.entry(node_path.clone()) {
                let invalid = {
                    let old_node = entry.get();
                    !old_node.is_instance_valid() || old_node.is_queued_for_deletion()
                };

                if invalid {
                    entry.remove();
                }
            }

            if !spawned_nodes.contains_key(&node_path) {
                if let Some(spawn_record) = frame.spawn_record(&node_path) {
                    nodes_to_spawn.push(spawn_record);
                }
            }
        }

        for node_to_spawn in nodes_to_spawn {
            self.spawn(owner, node_to_spawn, frame, true);
        }
    }
}
