use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use godot::prelude::*;
use parking_lot::RwLock;
use uuid::Uuid;

use super::input::Input;

#[derive(Clone)]
pub struct SpawnRecord {
    pub name: String,
    pub parent_path: String,
    pub scene: Gd<PackedScene>,
    pub state: Variant,
}

pub struct Frame {
    tick: u64,
    inputs: RwLock<HashMap<Uuid, Input>>,
    updated: AtomicBool,
    complete: AtomicBool,
    node_states: RwLock<HashMap<String, Variant>>,
    spawn_records: RwLock<HashMap<String, SpawnRecord>>,
    spawn_name_counters: RwLock<HashMap<String, usize>>,
    state_hash: AtomicU64,
}

impl Frame {
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            inputs: RwLock::new(HashMap::new()),
            updated: AtomicBool::new(false),
            complete: AtomicBool::new(false),
            node_states: RwLock::new(HashMap::new()),
            spawn_records: RwLock::new(HashMap::new()),
            spawn_name_counters: RwLock::new(HashMap::new()),
            state_hash: AtomicU64::new(0),
        }
    }

    pub fn initial_frame(peers: impl Iterator<Item = Uuid>) -> Self {
        let frame = Self::new(0);
        for peer in peers {
            frame.inputs.write().insert(peer, Input::default());
        }
        frame
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn input(&self, id: Uuid) -> Option<Input> {
        self.inputs.read().get(&id).cloned()
    }

    pub fn set_input(&self, id: Uuid, input: Input, peers: Vec<Uuid>) {
        self.inputs.write().insert(id, input);
        self.updated.store(true, Ordering::Relaxed);

        if self.inputs.read().len() == peers.len() {
            self.complete.store(true, Ordering::Relaxed);
        }
    }

    pub fn updated(&self) -> bool {
        self.updated.load(Ordering::Relaxed)
    }

    pub fn add_spawn_record(&self, node_path: String, spawn_record: SpawnRecord) {
        self.spawn_records.write().insert(node_path, spawn_record);
    }

    pub fn remove_spawn_record(&self, node_path: &str) {
        self.spawn_records.write().remove(node_path);
    }

    pub fn contains_spawn_record(&self, node_path: &str) -> bool {
        self.spawn_records.read().contains_key(node_path)
    }

    pub fn spawned_node_paths(&self) -> Vec<String> {
        self.spawn_records.read().keys().cloned().collect()
    }

    pub fn spawn_record(&self, node_path: &str) -> Option<SpawnRecord> {
        self.spawn_records.read().get(node_path).cloned()
    }

    pub fn copy_spawn_data(&self, frame: &Frame) {
        *self.spawn_records.write() = frame.spawn_records.read().clone();
        *self.spawn_name_counters.write() = frame.spawn_name_counters.read().clone();
    }

    pub fn node_state(&self, node_path: &str) -> Option<Variant> {
        self.node_states.read().get(node_path).cloned()
    }

    pub fn set_node_states(&self, node_states: HashMap<String, Variant>) {
        *self.node_states.write() = node_states;
        self.updated.store(false, Ordering::Relaxed);
    }

    pub fn avoid_name_collision(&self, name: String) -> String {
        let mut counters = self.spawn_name_counters.write();
        let counter = counters.entry(name.clone()).or_insert(0);
        *counter += 1;
        if counter == &1 {
            name
        } else {
            format!("{}{}", name, counter)
        }
    }

    pub fn missing_input<'a>(&self, peers: Vec<Uuid>) -> Option<Uuid> {
        let inputs = self.inputs.read();
        peers.iter().find(|id| !inputs.contains_key(&id)).cloned()
    }

    pub fn complete(&self) -> bool {
        self.complete.load(Ordering::Relaxed)
    }

    pub fn state_hash(&self) -> Option<u64> {
        if self.complete() {
            let hash = self.state_hash.load(Ordering::Relaxed);
            if hash != 0 {
                return Some(hash);
            }
        }

        None
    }

    pub fn set_state_hash(&self, state_hash: u64) {
        self.state_hash.store(state_hash, Ordering::Relaxed);
    }
}
