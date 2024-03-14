use std::{collections::HashMap, hash::Hash};

use anyhow::Result;
use indoc::indoc;
use rusqlite::{named_params, Connection};
use uuid::Uuid;

use crate::message::SentInput;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum LogEntry {
    RunInfo(RunInfo),
    SentInput(SentInput),
    ReceivedInput(ReceivedInput),
    DroppedFrame(DroppedFrame),
    Rollback(Rollback),
    FrameState(FrameState),
    SpawnedNodeAlive(SpawnedNodeAlive),
    Event(Event),
}

impl LogEntry {
    pub fn setup_tables(connection: &Connection) -> Result<()> {
        RunInfo::setup_table(connection)?;
        SentInput::setup_table(connection)?;
        ReceivedInput::setup_table(connection)?;
        DroppedFrame::setup_table(connection)?;
        Rollback::setup_table(connection)?;
        FrameState::setup_table(connection)?;
        SpawnedNodeAlive::setup_table(connection)?;
        Event::setup_table(connection)?;
        Ok(())
    }

    pub fn table_names() -> Vec<&'static str> {
        let mut table_names = Vec::new();
        table_names.append(&mut RunInfo::table_names());
        table_names.append(&mut SentInput::table_names());
        table_names.append(&mut ReceivedInput::table_names());
        table_names.append(&mut DroppedFrame::table_names());
        table_names.append(&mut Rollback::table_names());
        table_names.append(&mut FrameState::table_names());
        table_names.append(&mut SpawnedNodeAlive::table_names());
        table_names.append(&mut Event::table_names());
        table_names
    }

    pub fn frame(&self) -> u64 {
        match self {
            LogEntry::RunInfo(_) => 0,
            LogEntry::SentInput(SentInput { frame, .. }) => *frame,
            LogEntry::ReceivedInput(ReceivedInput { received_frame, .. }) => *received_frame,
            LogEntry::DroppedFrame(DroppedFrame { frame, .. }) => *frame,
            LogEntry::Rollback(Rollback { frame, .. }) => *frame,
            LogEntry::FrameState(FrameState { latest_frame, .. }) => *latest_frame,
            LogEntry::SpawnedNodeAlive(SpawnedNodeAlive { latest_frame, .. }) => *latest_frame,
            LogEntry::Event(Event { latest_frame, .. }) => *latest_frame,
        }
    }

    pub fn logger(&self) -> Uuid {
        match self {
            LogEntry::RunInfo(RunInfo { local_id, .. }) => *local_id,
            LogEntry::SentInput(SentInput { sender, .. }) => *sender,
            LogEntry::ReceivedInput(ReceivedInput { receiver, .. }) => *receiver,
            LogEntry::DroppedFrame(DroppedFrame { lagger, .. }) => *lagger,
            LogEntry::Rollback(Rollback { updater, .. }) => *updater,
            LogEntry::FrameState(FrameState { player, .. }) => *player,
            LogEntry::SpawnedNodeAlive(SpawnedNodeAlive { player, .. }) => *player,
            LogEntry::Event(Event { player, .. }) => *player,
        }
    }

    /// Writes the log entry to the given database connection.
    pub fn write(&self, connection: &Connection) -> Result<()> {
        match self {
            LogEntry::RunInfo(entry) => entry.write(connection),
            LogEntry::SentInput(entry) => entry.write(connection),
            LogEntry::ReceivedInput(entry) => entry.write(connection),
            LogEntry::DroppedFrame(entry) => entry.write(connection),
            LogEntry::Rollback(entry) => entry.write(connection),
            LogEntry::FrameState(entry) => entry.write(connection),
            LogEntry::SpawnedNodeAlive(entry) => entry.write(connection),
            LogEntry::Event(entry) => entry.write(connection),
        }
    }

    /// Reads all of the log entries from the given database connection.
    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut log_entries = Vec::new();

        log_entries.append(
            &mut RunInfo::read(connection)?
                .into_iter()
                .map(LogEntry::RunInfo)
                .collect(),
        );
        log_entries.append(
            &mut SentInput::read(connection)?
                .into_iter()
                .map(LogEntry::SentInput)
                .collect(),
        );
        log_entries.append(
            &mut ReceivedInput::read(connection)?
                .into_iter()
                .map(LogEntry::ReceivedInput)
                .collect(),
        );
        log_entries.append(
            &mut DroppedFrame::read(connection)?
                .into_iter()
                .map(LogEntry::DroppedFrame)
                .collect(),
        );
        log_entries.append(
            &mut Rollback::read(connection)?
                .into_iter()
                .map(LogEntry::Rollback)
                .collect(),
        );
        log_entries.append(
            &mut FrameState::read(connection)?
                .into_iter()
                .map(LogEntry::FrameState)
                .collect(),
        );
        log_entries.append(
            &mut SpawnedNodeAlive::read(connection)?
                .into_iter()
                .map(LogEntry::SpawnedNodeAlive)
                .collect(),
        );
        log_entries.append(
            &mut Event::read(connection)?
                .into_iter()
                .map(LogEntry::Event)
                .collect(),
        );

        log_entries.sort_by_key(|entry| entry.frame());

        Ok(log_entries)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunInfo {
    pub local_id: Uuid,
    pub peers: Vec<Uuid>,
}

impl RunInfo {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS run_info (
                local_id BLOB NOT NULL,  -- The id of the local player
                peer BLOB NOT NULL,    -- The id of a peer

                PRIMARY KEY (local_id, peer)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["run_info"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO run_info (local_id, peer)
                VALUES (:local_id, :peer)
        "})?;
        for peer in self.peers.iter() {
            statement.execute(named_params! {
                ":local_id": self.local_id.as_bytes(),
                ":peer": peer.as_bytes(),
            })?;
        }
        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection.prepare_cached(indoc! {"
            SELECT local_id, peer
            FROM run_info
        "})?;
        let local_id_peer_pairs = statement.query_and_then([], |row| -> Result<(Uuid, Uuid)> {
            Ok((
                Uuid::from_slice(&row.get::<_, Vec<u8>>(0)?)?,
                Uuid::from_slice(&row.get::<_, Vec<u8>>(1)?)?,
            ))
        })?;

        let mut run_infos = HashMap::new();
        for local_id_peer_pair in local_id_peer_pairs {
            let (local_id, peer) = local_id_peer_pair?;
            run_infos
                .entry(local_id)
                .or_insert_with(|| RunInfo {
                    local_id,
                    peers: Vec::new(),
                })
                .peers
                .push(peer);
        }
        Ok(run_infos.values().cloned().collect())
    }
}

impl SentInput {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS sent_inputs (
                frame INTEGER NOT NULL,   -- The frame the input is associated with
                sender BLOB NOT NULL,     -- The id of the sender of this input
                input BLOB NOT NULL,      -- The sent input
                PRIMARY KEY (frame, sender)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["sent_inputs"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO sent_inputs (frame, sender, input)
                VALUES (:frame, :sender, :input)
            "})?;

        statement.execute(named_params! {
            ":frame": self.frame,
            ":sender": self.sender.as_bytes(),
            ":input": bincode::serialize(&self.input)?,
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement =
            connection.prepare_cached("SELECT frame, sender, input FROM sent_inputs")?;

        let sent_inputs = statement
            .query_and_then([], |row| {
                let frame = row.get::<_, u64>(0)?;
                let sender = Uuid::from_slice(&row.get::<_, Vec<u8>>(1)?)?;
                let input = bincode::deserialize(&row.get::<_, Vec<u8>>(2)?)?;
                Ok(Self {
                    frame,
                    sender,
                    input,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        Ok(sent_inputs)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Hash)]
pub struct ReceivedInput {
    pub received_frame: u64,
    pub receiver: Uuid,
    pub sent_input: SentInput,
}

impl ReceivedInput {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS received_inputs (
                receiver BLOB NOT NULL,          -- The id of the receiver of this input
                received_frame INTEGER NOT NULL, -- The frame the input was received on
                sent_input BLOB NOT NULL,        -- The sent input that was received
                PRIMARY KEY (receiver, received_frame, sent_input)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["received_inputs"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO received_inputs (receiver, received_frame, sent_input)
                VALUES (:receiver, :received_frame, :sent_input)
            "})?;

        statement.execute(named_params! {
            ":receiver": self.receiver.as_bytes(),
            ":received_frame": self.received_frame,
            ":sent_input": bincode::serialize(&self.sent_input)?,
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection.prepare_cached(indoc! {"
                    SELECT receiver, received_frame, sent_input FROM received_inputs
                "})?;

        let inputs = statement.query_and_then([], |row| {
            let receiver = Uuid::from_slice(&row.get::<_, Vec<u8>>(0)?)?;
            let received_frame = row.get::<_, u64>(1)? as u64;
            let sent_input = bincode::deserialize(&row.get::<_, Vec<u8>>(2)?)?;
            Ok(Self {
                received_frame,
                receiver,
                sent_input,
            })
        })?;

        inputs.collect()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Hash)]
pub struct DroppedFrame {
    pub id: usize,
    pub frame: u64,
    pub frame_missing_input: u64,
    pub lagger: Uuid,
    pub dropper: Uuid,
}

impl DroppedFrame {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS dropped_frames (
                id INTEGER NOT NULL,                  -- Monotonically increasing id by logger
                frame INTEGER NOT NULL,               -- The frame that was dropped
                frame_missing_input INTEGER NOT NULL, -- The frame that was missing input
                lagger BLOB NOT NULL,                 -- The id of the player that was lagging
                dropper BLOB NOT NULL,                -- The id of the peer that dropped the frame
                PRIMARY KEY (id, dropper)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["dropped_frames"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO dropped_frames (id, frame, frame_missing_input, lagger, dropper)
                VALUES (:id, :frame, :frame_missing_input, :lagger, :dropper)
            "})?;

        statement.execute(named_params! {
            ":id": self.id,
            ":frame": self.frame,
            ":frame_missing_input": self.frame_missing_input,
            ":lagger": self.lagger.as_bytes(),
            ":dropper": self.dropper.as_bytes(),
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection.prepare_cached(
            "SELECT id, frame, frame_missing_input, lagger, dropper FROM dropped_frames",
        )?;

        let frames = statement.query_and_then([], |row| {
            let id = row.get::<_, usize>(0)?;
            let frame = row.get::<_, u64>(1)?;
            let frame_missing_input = row.get::<_, u64>(2)?;
            let lagger = Uuid::from_slice(&row.get::<_, Vec<u8>>(3)?)?;
            let dropper = Uuid::from_slice(&row.get::<_, Vec<u8>>(4)?)?;
            Ok(DroppedFrame {
                id,
                frame,
                frame_missing_input,
                lagger,
                dropper,
            })
        })?;

        frames.collect()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Hash)]
pub struct Rollback {
    pub frame: u64,
    pub rolled_back_to: u64,
    pub updater: Uuid,
}

impl Rollback {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS rollbacks (
                frame INTEGER NOT NULL,
                rolled_back_to INTEGER NOT NULL,
                updater BLOB NOT NULL,
                PRIMARY KEY (frame, updater)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["rollbacks"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO rollbacks (frame, rolled_back_to, updater)
                VALUES (:frame, :rolled_back_to, :updater)
            "})?;

        statement.execute(named_params! {
            ":frame": self.frame,
            ":rolled_back_to": self.rolled_back_to,
            ":updater": self.updater.as_bytes(),
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement =
            connection.prepare_cached("SELECT frame, rolled_back_to, updater FROM rollbacks")?;

        let rollbacks = statement.query_and_then([], |row| {
            let frame = row.get::<_, u64>(0)?;
            let rolled_back_to = row.get::<_, u64>(1)?;
            let updater = Uuid::from_slice(&row.get::<_, Vec<u8>>(2)?)?;
            Ok(Self {
                frame,
                rolled_back_to,
                updater,
            })
        })?;

        rollbacks.collect()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Hash, PartialEq, Eq)]
pub struct FrameState {
    pub frame: u64,
    pub latest_frame: u64,
    pub player: Uuid,
    pub path: String,
    pub key: String,
    pub value_text: String,
    pub value_hash: u64,
}

impl FrameState {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS frame_states (
                frame INTEGER NOT NULL,
                latest_frame INTEGER NOT NULL,
                player BLOB NOT NULL,
                path TEXT NOT NULL,
                key TEXT NOT NULL,
                value_text TEXT NOT NULL,
                value_hash BLOB NOT NULL,
                PRIMARY KEY (frame, latest_frame, player, path, key)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["frame_states"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT OR REPLACE INTO frame_states (frame, latest_frame, player, path, key, value_text, value_hash)
                VALUES (:frame, :latest_frame, :player, :path, :key, :value_text, :value_hash)
            "})?;

        let value_hash_bytes = self.value_hash.to_be_bytes();

        statement.execute(named_params! {
            ":frame": self.frame,
            ":latest_frame": self.latest_frame,
            ":player": self.player.as_bytes(),
            ":path": self.path,
            ":key": self.key,
            ":value_text": self.value_text,
            ":value_hash": value_hash_bytes,
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection.prepare_cached(
            "SELECT frame, latest_frame, player, path, key, value_text, value_hash FROM frame_states",
        )?;

        let states = statement.query_and_then([], |row| {
            let frame = row.get::<_, u64>(0)?;
            let latest_frame = row.get::<_, u64>(1)?;
            let player = Uuid::from_slice(&row.get::<_, Vec<u8>>(2)?)?;
            let path = row.get::<_, String>(3)?;
            let key = row.get::<_, String>(4)?;
            let value_text = row.get::<_, String>(5)?;
            let value_hash_bytes: [u8; 8] = row.get::<_, Vec<u8>>(6)?.try_into().unwrap();
            let value_hash = u64::from_be_bytes(value_hash_bytes);
            Ok(Self {
                frame,
                latest_frame,
                player,
                path,
                key,
                value_text,
                value_hash,
            })
        })?;

        states.collect()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Hash, PartialEq, Eq)]
pub struct SpawnedNodeAlive {
    pub frame: u64,
    pub latest_frame: u64,
    pub player: Uuid,
    pub node_path: String,
}

impl SpawnedNodeAlive {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS spawned_nodes (
                frame INTEGER NOT NULL,
                latest_frame INTEGER NOT NULL,
                player BLOB NOT NULL,
                node_path TEXT NOT NULL,
                PRIMARY KEY (frame, latest_frame, player, node_path)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["spawned_nodes"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT OR REPLACE INTO spawned_nodes (frame, latest_frame, player, node_path)
                VALUES (:frame, :latest_frame, :player, :node_path)
            "})?;

        statement.execute(named_params! {
            ":frame": self.frame,
            ":latest_frame": self.latest_frame,
            ":player": self.player.as_bytes(),
            ":node_path": self.node_path,
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection
            .prepare_cached("SELECT frame, latest_frame, player, node_path FROM spawned_nodes")?;

        let states = statement.query_and_then([], |row| {
            let frame = row.get::<_, u64>(0)?;
            let latest_frame = row.get::<_, u64>(1)?;
            let player = Uuid::from_slice(&row.get::<_, Vec<u8>>(2)?)?;
            let node_path = row.get::<_, String>(3)?;
            Ok(Self {
                frame,
                latest_frame,
                player,
                node_path,
            })
        })?;

        states.collect()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct Event {
    pub id: usize,
    pub frame: u64,
    pub latest_frame: u64,
    pub player: Uuid,
    pub event: String,
    pub data: String,
}

impl Event {
    pub fn setup_table(connection: &Connection) -> Result<()> {
        connection.execute_batch(indoc! {"
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER NOT NULL,
                frame INTEGER NOT NULL,
                latest_frame INTEGER NOT NULL,
                player BLOB NOT NULL,
                event TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (id, player)
            );
        "})?;
        Ok(())
    }

    fn table_names() -> Vec<&'static str> {
        vec!["events"]
    }

    pub fn write(&self, connection: &Connection) -> Result<()> {
        let mut statement = connection.prepare_cached(indoc! {"
                INSERT INTO events (id, frame, latest_frame, player, event, data)
                VALUES (:id, :frame, :latest_frame, :player, :event, :data)
            "})?;

        statement.execute(named_params! {
            ":id": self.id,
            ":frame": self.frame,
            ":latest_frame": self.latest_frame,
            ":player": self.player.as_bytes(),
            ":event": self.event,
            ":data": self.data,
        })?;

        Ok(())
    }

    pub fn read(connection: &Connection) -> Result<Vec<Self>> {
        let mut statement = connection
            .prepare_cached("SELECT id, frame, latest_frame, player, event, data FROM events")?;

        let states = statement.query_and_then([], |row| {
            let id = row.get::<_, usize>(0)?;
            let frame = row.get::<_, u64>(1)?;
            let latest_frame = row.get::<_, u64>(2)?;
            let player = Uuid::from_slice(&row.get::<_, Vec<u8>>(3)?)?;
            let event = row.get::<_, String>(4)?;
            let data = row.get::<_, String>(5)?;
            Ok(Self {
                id,
                frame,
                latest_frame,
                player,
                event,
                data,
            })
        })?;

        states.collect()
    }
}
