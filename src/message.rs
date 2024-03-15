use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SentInput {
    pub frame: u64,
    pub sender: Uuid,
    pub input: Vec<u8>,
}

impl Hash for SentInput {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.frame.hash(state);
        self.sender.hash(state);
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum Message {
    // If uuid is not in peers, add it, send a connect in reply and gossip the address to all
    // other peers
    Connect(Uuid),
    // Send a connect message to the address if it is not in peers
    GossipPeer(Uuid, String),
    // Mark the peer with the value. If all peers are ready, and your
    // id is lowest, send a schedule start message to all peers
    UpdateReady(bool),
    // Schedule the start in 5 seconds
    ScheduleStart(Uuid),
    // Store the input in the input table for the given frame and id
    Input {
        sent_input: SentInput,
        last_received_frame: u64,
    },
    // Compare the given hash with the stored state hash for the given frame
    // If they do not mash, there has been a desync
    StateHash {
        frame: u64,
        hash: u64,
    },
}
