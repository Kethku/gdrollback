use std::collections::{BTreeMap, BTreeSet, HashMap};

use gdrollback::{
    logging::{
        DroppedFrame, Event, FrameState, ReceivedInput, Rollback, RunInfo, SpawnedNodeAlive,
    },
    SentInput,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct PlayerEntries {
    pub run_info: Option<RunInfo>,
    pub sent_input: Option<SentInput>,
    pub received_inputs: Vec<ReceivedInput>,
    pub rollback: Option<Rollback>,
    pub dropped_frame: Option<DroppedFrame>,
    pub frame_states: Vec<FrameState>,
    pub spawned_nodes_alive: HashMap<u64, Vec<SpawnedNodeAlive>>,
    pub events: HashMap<u64, BTreeSet<Event>>,
}

impl PlayerEntries {
    pub fn contains_state(&self, highlighted_state: &Option<(String, String, u64)>) -> bool {
        let Some((expected_path, expected_key, expected_hash)) = highlighted_state.as_ref() else {
            return false;
        };

        for state in self.frame_states.iter() {
            if &state.path == expected_path
                && &state.key == expected_key
                && &state.value_hash == expected_hash
            {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug, Hash)]
pub struct Argument {
    pub player: Uuid,
    pub state: Option<FrameState>,
}

#[derive(Clone, Debug, Hash)]
pub enum SyncState {
    Synced {
        consensus: Vec<FrameState>,
    },
    Desynced {
        disagreements: BTreeMap<(String, String), Vec<Argument>>,
    },
}

impl SyncState {
    pub fn contains_state(&self, highlighted_state: &Option<(String, String, u64)>) -> bool {
        let Some((expected_path, expected_key, expected_hash)) = highlighted_state.as_ref() else {
            return false;
        };

        match self {
            SyncState::Synced { consensus } => {
                for FrameState {
                    path,
                    key,
                    value_hash,
                    ..
                } in consensus
                {
                    if path == expected_path && key == expected_key && value_hash == expected_hash {
                        return true;
                    }
                }
                false
            }
            SyncState::Desynced { disagreements } => {
                for ((path, key), arguments) in disagreements {
                    if path == expected_path && key == expected_key {
                        for argument in arguments.iter() {
                            if let Some(FrameState { value_hash, .. }) = argument.state {
                                if value_hash == *expected_hash {
                                    return true;
                                }
                            }
                        }
                    }
                }
                false
            }
        }
    }
}

#[derive(Clone)]
pub struct FrameEntries {
    pub player_entries: HashMap<Uuid, PlayerEntries>,
    pub sync_state: SyncState,
}
