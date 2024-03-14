use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    time::SystemTime,
};

use anyhow::Result;
use egui::{Color32, Label, RichText, Sense};
use itertools::Itertools;
use uuid::Uuid;

use gdrollback::logging::{FrameState, LogEntry, LogReader};

use crate::{
    entries::{Argument, FrameEntries, PlayerEntries, SyncState},
    util::{small_text, trim_path},
};

const PLAYER_COLORS: &[Color32] = &[
    Color32::RED,
    Color32::GREEN,
    Color32::BLUE,
    Color32::YELLOW,
    Color32::LIGHT_RED,
    Color32::LIGHT_GREEN,
    Color32::LIGHT_BLUE,
    Color32::LIGHT_YELLOW,
];

pub struct Run {
    pub log_reader: Option<LogReader>,
    pub id: Uuid,
    pub players: Vec<Uuid>,
    pub frames: HashMap<u64, FrameEntries>,
    pub edited: SystemTime,
    pub highlighted_state: Option<(String, String, u64)>,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            log_reader: None,
            id: Uuid::nil(),
            players: Vec::new(),
            frames: HashMap::new(),
            edited: SystemTime::UNIX_EPOCH,
            highlighted_state: None,
        }
    }
}

impl Run {
    pub fn new(run_id: Uuid, edited: SystemTime) -> Result<Self> {
        Ok(Self {
            log_reader: Some(LogReader::load_run(run_id)?),
            id: run_id,
            edited,
            ..Default::default()
        })
    }

    /// Panics if the player is not in the run.
    pub fn player_number(&self, player: Uuid) -> usize {
        self.players
            .iter()
            .position(|p| p == &player)
            .unwrap_or_else(|| panic!("Player {:?} is not in the run", player))
    }

    pub fn player_label(&self, player: Uuid) -> RichText {
        let player_number = self.player_number(player);

        RichText::new(player_number.to_string()).color(*PLAYER_COLORS.get(player_number).unwrap())
    }

    pub fn state_label(
        &mut self,
        ui: &mut egui::Ui,
        state @ FrameState { path, key, .. }: &FrameState,
    ) {
        let path_text = trim_path(path);

        ui.horizontal(|ui| {
            ui.label(format!("{path_text}::{key}:"));
            self.state_value_label(ui, state);
        });
    }

    pub fn state_value_label(
        &mut self,
        ui: &mut egui::Ui,
        FrameState {
            path,
            key,
            value_text,
            value_hash,
            ..
        }: &FrameState,
    ) {
        let hash_text = small_text(*value_hash);
        if ui
            .add(Label::new(format!("{value_text}#{hash_text}")).sense(Sense::click()))
            .clicked()
        {
            let clicked_state = Some((path.clone(), key.clone(), *value_hash));
            if self.highlighted_state == clicked_state {
                self.highlighted_state = None;
            } else {
                self.highlighted_state = clicked_state;
            }
        }
    }

    pub fn update_data(&mut self) -> Result<()> {
        let log_reader = if let Some(log_reader) = self.log_reader.as_ref() {
            log_reader
        } else {
            self.log_reader = Some(LogReader::load_run(self.id)?);
            self.log_reader.as_ref().unwrap()
        };

        self.players = log_reader.players()?;
        let entries_by_frame = log_reader
            .log_entries()?
            .into_iter()
            .into_group_map_by(|entry| entry.frame());
        let frame_count = log_reader.frame_count()?;

        self.frames.clear();
        for frame in 0..frame_count {
            let Some(entries) = entries_by_frame.get(&(frame as u64)) else {
                continue;
            };

            let mut frame_entries = FrameEntries {
                player_entries: HashMap::new(),
                sync_state: SyncState::Synced {
                    consensus: Default::default(),
                },
            };
            let entries_by_player: HashMap<Uuid, Vec<LogEntry>> = entries
                .into_iter()
                .into_group_map_by(|entry| entry.logger())
                .into_iter()
                .map(|(logger, entries)| {
                    (
                        logger,
                        entries.into_iter().cloned().collect::<Vec<LogEntry>>(),
                    )
                })
                .collect();

            for logger in self.players.iter() {
                let mut player_entries = PlayerEntries {
                    run_info: None,
                    sent_input: None,
                    received_inputs: Vec::new(),
                    rollback: None,
                    dropped_frame: None,
                    frame_states: Vec::new(),
                    spawned_nodes_alive: HashMap::new(),
                    events: HashMap::new(),
                };

                for entry in entries_by_player.get(logger).unwrap_or(&Vec::new()) {
                    match entry {
                        LogEntry::RunInfo(entry) => {
                            player_entries.run_info = Some(entry.clone());
                        }
                        LogEntry::SentInput(entry) => {
                            player_entries.sent_input = Some(entry.clone());
                        }
                        LogEntry::ReceivedInput(entry) => {
                            player_entries.received_inputs.push(entry.clone());
                        }
                        LogEntry::Rollback(entry) => {
                            player_entries.rollback = Some(entry.clone());
                        }
                        LogEntry::DroppedFrame(entry) => {
                            player_entries.dropped_frame = Some(entry.clone());
                        }
                        LogEntry::FrameState(entry) => {
                            player_entries.frame_states.push(entry.clone());
                        }
                        LogEntry::SpawnedNodeAlive(entry) => {
                            player_entries
                                .spawned_nodes_alive
                                .entry(entry.frame)
                                .or_insert_with(Vec::new)
                                .push(entry.clone());
                        }
                        LogEntry::Event(entry) => {
                            player_entries
                                .events
                                .entry(entry.frame)
                                .or_insert_with(BTreeSet::new)
                                .insert(entry.clone());
                        }
                    }
                }

                frame_entries.player_entries.insert(*logger, player_entries);
            }

            // Compute desyncs
            let latest_states = self
                .players
                .iter()
                .filter_map(|player| {
                    log_reader
                        .latest_states_for_frame(*player, frame)
                        .ok()
                        .map(|states| (*player, states))
                })
                .collect::<Vec<_>>();

            if let Some((_, sentinel)) = latest_states.get(0) {
                if latest_states.iter().all(|(_, states)| {
                    states
                        .iter()
                        .zip(sentinel.iter())
                        .all(|(a, b)| a.value_hash == b.value_hash)
                }) {
                    frame_entries.sync_state = SyncState::Synced {
                        consensus: sentinel.clone(),
                    };
                } else {
                    let mut disagreements = BTreeMap::new();
                    let keys: HashSet<(String, String)> = latest_states
                        .iter()
                        .flat_map(|(_, states)| {
                            states
                                .iter()
                                .map(|state| (state.path.clone(), state.key.clone()))
                        })
                        .collect();
                    for (path, key) in keys.into_iter() {
                        for (player, player_states) in latest_states.iter() {
                            let state = player_states
                                .iter()
                                .find(|state| state.path == path && state.key == key)
                                .cloned();
                            let arguments = disagreements
                                .entry((path.clone(), key.clone()))
                                .or_insert_with(|| Vec::new());
                            arguments.push(Argument {
                                player: *player,
                                state,
                            });
                        }
                    }
                    disagreements.retain(|_, arguments| {
                        let first = arguments[0].state.as_ref().map(|state| state.value_hash);
                        arguments.iter().any(|argument| {
                            argument.state.as_ref().map(|state| state.value_hash) != first
                        })
                    });
                    frame_entries.sync_state = SyncState::Desynced { disagreements };
                }
            }
            self.frames.insert(frame, frame_entries);
        }

        Ok(())
    }
}
