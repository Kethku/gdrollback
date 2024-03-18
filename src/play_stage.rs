mod frame;
mod spawn_manager;

use std::{
    collections::{hash_map::DefaultHasher, HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use godot::{
    engine::utilities::{bytes_to_var, var_to_bytes},
    prelude::*,
};
use uuid::Uuid;

use crate::{
    context::Context,
    message::{Message, SentInput},
    replay_stage::ReplayStage,
    sync_manager::RollbackSyncManager,
    sync_stage::SyncStage,
};
use frame::{Frame, SpawnRecord};

use self::spawn_manager::SpawnManager;

const MAX_REWIND: u64 = 30;

pub struct PlayStage {
    frames: HashMap<u64, Arc<Frame>>,
    spawn_manager: Arc<SpawnManager>,
    /// Contains the last input tick received by each remote peer
    latest_frame_delivered: HashMap<Uuid, u64>,
    /// Contains the last input tick recieved from each remote peer
    latest_frame_received: HashMap<Uuid, u64>,
    rolling_advantage_sum: i64,
    advantage_queue: VecDeque<i64>,
}

impl PlayStage {
    pub fn new(early_inputs: Vec<Message>, cx: &Context) -> Self {
        let peers = cx.peers();
        // Initialize the first 2 frames with default inputs to ensure no
        // rollbacks
        let mut frames = HashMap::new();
        frames.insert(0, Arc::new(Frame::initial_frame(peers.iter().copied())));
        frames.insert(1, Arc::new(Frame::initial_frame(peers.iter().copied())));

        let mut this = Self {
            frames,
            spawn_manager: Arc::new(SpawnManager::new()),
            latest_frame_delivered: HashMap::new(),
            latest_frame_received: HashMap::new(),
            rolling_advantage_sum: 0,
            advantage_queue: VecDeque::new(),
        };

        for message in early_inputs {
            this.handle_message(message, cx)
                .expect("Couldn't handle message");
        }

        this
    }

    pub fn input(&self, id: String, cx: &Context) -> Variant {
        let id = Uuid::parse_str(&id).unwrap();
        for tick in (cx.latest_tick().saturating_sub(MAX_REWIND)..=cx.current_tick()).rev() {
            if let Some(frame) = self.frames.get(&tick) {
                if let Some(input) = frame.input(id) {
                    return input;
                }
            }
        }
        Default::default()
    }

    pub fn advantage(&self) -> f64 {
        self.rolling_advantage_sum as f64 / self.advantage_queue.len() as f64
    }

    pub fn tick(&mut self, node: &Gd<Node>, cx: &Context) -> Result<Option<SyncStage>> {
        let mut largest_advantage: Option<i64> = None;

        for peer in cx.peers() {
            let latest_frame_received =
                self.latest_frame_received.get(&peer).copied().unwrap_or(0) as i64;
            let remote_frame_lag = latest_frame_received
                - self.latest_frame_delivered.get(&peer).copied().unwrap_or(0) as i64;
            let local_frame_lag = cx.latest_tick() as i64 - latest_frame_received as i64;

            largest_advantage = match largest_advantage {
                Some(largest_advantage) => {
                    Some(largest_advantage.max(local_frame_lag - remote_frame_lag))
                }
                None => Some(local_frame_lag - remote_frame_lag),
            };
        }

        if let Some(largest_advantage) = largest_advantage {
            self.rolling_advantage_sum += largest_advantage;
            self.advantage_queue.push_back(largest_advantage);
            if self.advantage_queue.len() > 100 {
                self.rolling_advantage_sum -= self.advantage_queue.pop_front().unwrap();
            }
        }

        let node = (*node).clone();
        let mut sync_manager = node.cast::<RollbackSyncManager>();
        sync_manager.call_deferred("execute_tick".into(), &[]);
        Ok(None)
    }

    pub fn handle_message(&mut self, message: Message, cx: &Context) -> Result<()> {
        match &message {
            Message::Input {
                sent_input:
                    sent_input @ SentInput {
                        frame: tick,
                        sender: remote_id,
                        input,
                    },
                last_received_frame: new_latest_frame_delivered,
            } => {
                // Store the input in the input table for the given frame and id
                cx.logger()
                    .received_input(cx.latest_tick() + 1, sent_input.clone(), cx)?;
                let frame = self
                    .frames
                    .entry(*tick)
                    .or_insert_with(|| Arc::new(Frame::new(*tick)));
                self.latest_frame_delivered.insert(*remote_id, *tick);
                frame.set_input(
                    *remote_id,
                    bytes_to_var(PackedByteArray::from(&input[..])),
                    cx.peers(),
                );

                let latest_frame_received =
                    self.latest_frame_received.entry(*remote_id).or_insert(0);
                *latest_frame_received = (*latest_frame_received).max(*tick);

                let latest_frame_delivered =
                    self.latest_frame_delivered.entry(*remote_id).or_insert(0);
                *latest_frame_delivered =
                    (*latest_frame_delivered).max(*new_latest_frame_delivered);
            }
            Message::StateHash {
                frame: tick,
                hash: remote_hash,
            } => {
                if let Some(frame) = self.frames.get(tick) {
                    if let Some(local_hash) = frame.state_hash() {
                        if *remote_hash != local_hash {
                            panic!("Desync detected at tick {tick} {remote_hash} != {local_hash}");
                        }
                    }
                }
            }
            _ => panic!("Recieved lobby message during play stage"),
        }

        Ok(())
    }

    pub fn execute_tick(mut owner: impl PlayStageOwner) {
        let peers = owner.peers();
        let Some((oldest_updated, latest_tick)) = owner.update(|this, cx| {
            // Remove frames that are older than the rewind max
            let oldest_tick = (cx.latest_tick() + 1).saturating_sub(MAX_REWIND);
            for old_tick in this
                .frames
                .keys()
                .copied()
                .filter(|tick| tick < &oldest_tick)
                .collect::<Vec<_>>()
            {
                let frame = this.frames.remove(&old_tick).unwrap();
                if let Some(missing_input_peer) = frame.missing_input(peers.clone()) {
                    // This frame is missing input from one of the peers.
                    // Log that we are stalling in order for the peer to catch up
                    // and add it back.
                    cx.logger()
                        .dropped_frame(cx.latest_tick() + 1, old_tick, missing_input_peer, cx)
                        .unwrap();
                    this.frames.insert(old_tick, frame);
                    return None;
                }

                // TODO: Maybe notify nodes that this tick is dead now
                // Could be useful for when a node doesn't return the entire state
                // and instead returns a state id

                let simulation_frame_advantage = this.advantage() / 2.0;
                if simulation_frame_advantage >= 0.75 {
                    let period = ((MAX_REWIND / 2) as f64 - (simulation_frame_advantage + 0.5))
                        .max(1.0) as u64
                        * 3;
                    if cx.latest_tick() % period == 0 {
                        // Stall a frame to let other peers catch up
                        return None;
                    }
                }
            }

            let latest_tick = cx.increment_latest_tick();

            this.frames
                .entry(latest_tick)
                .or_insert_with(|| Arc::new(Frame::new(latest_tick)));

            // Find the latest tick without any updates before it
            let mut oldest_updated = latest_tick;
            for tick in oldest_tick..latest_tick {
                if let Some(frame) = this.frames.get(&tick) {
                    if frame.updated() {
                        oldest_updated = tick;
                        break;
                    }
                }
            }

            Some((oldest_updated, latest_tick))
        }) else {
            return;
        };

        // Load the frame before the oldest_updated if a rollback was necessary
        if oldest_updated != latest_tick {
            let frame_to_load = oldest_updated.saturating_sub(1);
            owner.update(|_, cx| {
                cx.set_current_tick(frame_to_load);
                cx.logger()
                    .rollback(latest_tick, frame_to_load, cx)
                    .unwrap();
            });
            owner.load_frame(frame_to_load);
        }

        // Dont record input on the first tick to ensure we have something
        // to roll back to
        if latest_tick > 1 {
            let new_input = owner.fetch_local_input();
            let (sent_input, latest_frame_received) = owner.update(|this, cx| {
                let sent_input = SentInput {
                    frame: latest_tick,
                    sender: cx.local_id(),
                    input: var_to_bytes(new_input.clone()).to_vec(),
                };

                cx.logger()
                    .sent_input(sent_input.clone())
                    .expect("Couldn't log sent input");
                let frame = this.frames.get_mut(&latest_tick).unwrap();
                frame.set_input(cx.local_id(), new_input.clone(), cx.peers());
                (sent_input, this.latest_frame_received.clone())
            });

            for id in owner.peers() {
                let message = Message::Input {
                    sent_input: sent_input.clone(),
                    last_received_frame: latest_frame_received.get(&id).copied().unwrap_or(0),
                };

                owner.send(id, message);
            }
        }

        for tick in oldest_updated.min(latest_tick)..=latest_tick {
            owner.update(|this, cx| {
                let frame = this.frames.get(&tick).unwrap();
                if let Some(previous_frame) = this.frames.get(&tick.saturating_sub(1)) {
                    frame.copy_spawn_data(&previous_frame);
                }
                cx.set_current_tick(tick);
            });

            let new_state = owner.networked_process();
            let state_hash = owner.log_node_states();

            owner.update(|this, cx| {
                if let Some(state_hash) = state_hash {
                    cx.broadcast(Message::StateHash {
                        frame: tick,
                        hash: state_hash,
                    })
                    .unwrap();
                }

                let frame = this.frames.get(&tick).unwrap();
                frame.set_node_states(new_state);
                for spawned_node_path in frame.spawned_node_paths() {
                    cx.logger()
                        .spawned_node_alive(spawned_node_path, cx)
                        .unwrap();
                }
            });
        }
    }

    pub fn despawn(mut owner: impl PlayStageOwner, node: &Gd<Node>) {
        let (frame, spawn_manager) = owner.update(|this, cx| {
            let frame = this.frames.get(&cx.current_tick()).unwrap();
            (frame.clone(), this.spawn_manager.clone())
        });

        spawn_manager.despawn(&mut owner, &node.get_path().to_string(), frame.as_ref());
    }

    pub fn spawn(
        mut owner: impl PlayStageOwner,
        name: String,
        parent: &Gd<Node>,
        scene: Gd<PackedScene>,
        state: Variant,
    ) -> Gd<Node> {
        let (frame, spawn_manager) = owner.update(|this, cx| {
            let frame = this.frames.get(&cx.current_tick()).unwrap();
            (frame.clone(), this.spawn_manager.clone())
        });
        let parent_path = parent.get_path().to_string();
        let spawn_record = SpawnRecord {
            name,
            parent_path,
            scene,
            state,
        };
        spawn_manager.spawn(&mut owner, spawn_record, frame.as_ref(), false)
    }
}

// Trait implemented by the owner of the play stage. This is used in
// execute_tick so that mutability of the play_stage can be dynamically
// acquired and revoked while script code is running.
pub trait PlayStageOwner {
    // Update the play stage from the owner with a callback that gets a mutable reference
    fn update<T, CB: FnOnce(&mut PlayStage, &mut Context) -> T>(&mut self, callback: CB) -> T;
    // Loads the frame for the given tick into all networked nodes and
    // spawns/despawns whatever nodes necessary to return to that frame's state
    fn load_frame(&mut self, tick: u64);
    // Fetches the local input
    fn fetch_local_input(&mut self) -> Variant;
    // Sends a serializable message to a specific peer
    fn send(&mut self, peer: Uuid, message: Message);
    // Returns the list of peers that are currently connected
    fn peers(&self) -> Vec<Uuid>;
    // Calls networked_process on all networked nodes returning their updated states
    fn networked_process(&mut self) -> HashMap<String, Variant>;
    // Calls log_state on all networked nodes and logs the result to the logger
    fn log_node_states(&mut self) -> Option<u64>;
    // Gets a node from the node tree
    fn get_node(&self, path: &str) -> Option<Gd<Node>>;
}

impl PlayStageOwner for Gd<RollbackSyncManager> {
    fn update<T, CB: FnOnce(&mut PlayStage, &mut Context) -> T>(&mut self, callback: CB) -> T {
        let sync_manager: &mut RollbackSyncManager = &mut self.bind_mut();
        match &mut sync_manager.stage {
            SyncStage::Play(this) => callback(this, &mut sync_manager.context),
            SyncStage::Replay(ReplayStage { play_stage, .. }) => {
                callback(play_stage, &mut sync_manager.context)
            }
            _ => panic!("Tried to execute tick on non-play stage"),
        }
    }

    fn load_frame(&mut self, tick: u64) {
        let (networked_nodes, spawn_manager, frame) = {
            let networked_nodes = self
                .get_tree()
                .expect("Couldn't get tree")
                .get_nodes_in_group("networked".into());

            let (spawn_manager, frame) = self.update(|this, _| {
                (
                    this.spawn_manager.clone(),
                    this.frames.get(&tick).unwrap().clone(),
                )
            });
            (networked_nodes, spawn_manager, frame)
        };

        // Load the frame state into all networked nodes
        for mut networked_node in networked_nodes.iter_shared() {
            if networked_node.has_method("load_state".into()) {
                if let Some(node_state) = frame.node_state(&networked_node.get_path().to_string()) {
                    networked_node.call("load_state".into(), &[node_state.clone()]);
                }
            }
        }

        // Spawn or despawn nodes to match the frame state
        spawn_manager.load_frame(self, frame.as_ref());
    }

    fn fetch_local_input(&mut self) -> Variant {
        {
            let sync_manager = self.bind();
            if let SyncStage::Replay(replay_stage) = &sync_manager.stage {
                return replay_stage.local_input(&sync_manager.context);
            }
        }

        let mut input_manager = self.get_node("/root/InputManager".into()).unwrap();
        input_manager.call("networked_input".into(), &[])
    }

    fn send(&mut self, peer: Uuid, message: Message) {
        let mut sync_manager = self.bind_mut();
        sync_manager
            .context
            .send_to(peer, message)
            .expect("Couldn't broadcast message");
    }

    fn peers(&self) -> Vec<Uuid> {
        let sync_manager = self.bind();
        sync_manager.context.peers()
    }

    fn networked_process(&mut self) -> HashMap<String, Variant> {
        let networked_nodes = self
            .get_tree()
            .expect("Couldn't get tree")
            .get_nodes_in_group("networked".into());

        for mut networked_node in networked_nodes.iter_shared() {
            if networked_node.has_method("networked_preprocess".into()) {
                networked_node.call("networked_preprocess".into(), &[]);
            }
        }

        let mut node_states = HashMap::new();
        for mut networked_node in networked_nodes.iter_shared() {
            if networked_node.has_method("networked_process".into()) {
                let path = networked_node.get_path().to_string();
                let new_state = networked_node.call("networked_process".into(), &[]);
                node_states.insert(path, new_state);
            }
        }

        node_states
    }

    // If the current frame is complete, returns a hash over all of the node states
    // in the frame for desync detection purposes. Otherwise, returns None.
    fn log_node_states(&mut self) -> Option<u64> {
        let networked_nodes = self
            .get_tree()
            .expect("Couldn't get tree")
            .get_nodes_in_group("networked".into());

        let mut combined_hasher = self.update(|this, cx| {
            let frame = this.frames.get(&cx.current_tick()).unwrap();
            if frame.missing_input(cx.peers()).is_none() {
                Some(DefaultHasher::new())
            } else {
                None
            }
        });

        for mut networked_node in networked_nodes.iter_shared() {
            if networked_node.has_method("log_state".into()) {
                let path = networked_node.get_path().to_string();
                let states_variant = networked_node.call("log_state".into(), &[]);
                // Convert states variant to a dictionary of key and value strings
                if let Ok(states) = states_variant.try_to::<Dictionary>() {
                    for (key, value) in states.iter_shared() {
                        let key = key.stringify().to_string();
                        let value_text = value.stringify().to_string();
                        let value_bytes = utilities::var_to_bytes(value);
                        let value_bytes = value_bytes.as_slice();
                        let mut hasher = DefaultHasher::new();
                        value_bytes.hash(&mut hasher);
                        if let Some(combined) = combined_hasher.as_mut() {
                            value_bytes.hash(combined);
                        }

                        {
                            let cx = &self.bind().context;
                            let value_hash = hasher.finish();
                            cx.logger()
                                .state(path.clone(), key, value_text, value_hash, cx)
                                .unwrap();
                        }
                    }
                }
            }
        }

        if let Some(hasher) = combined_hasher.as_mut() {
            let state_hash = hasher.finish();
            self.update(|this, cx| {
                this.frames
                    .get_mut(&cx.current_tick())
                    .unwrap()
                    .set_state_hash(state_hash);
            });
            Some(state_hash)
        } else {
            None
        }
    }

    fn get_node(&self, path: &str) -> Option<Gd<Node>> {
        self.clone().upcast::<Node>().get_node(path.into())
    }
}
