use std::collections::BTreeSet;

use egui::{CentralPanel, Color32, Grid, RichText, ScrollArea, Separator, Window};
use egui_phosphor::fill;
use gdrollback::SentInput;
use itertools::Itertools;

use crate::{entries::SyncState, util::trim_path, window_button::UiExt, App};

pub fn show_content(app: &mut App, ctx: &egui::Context) {
    CentralPanel::default().show(ctx, |ui| {
        let Some(run) = app.runs.get_mut(app.focused_run_index) else {
            ui.centered_and_justified(|ui| {
                ui.heading("No runs found");
            });
            return;
        };

        Window::new("State").show(ctx, |ui| {
            if let Some((path, key, hash)) = &run.highlighted_state {
                ui.label(format!("{}: {}", path, key));
                ui.label(format!("Hash: {}", hash));
            } else {
                ui.label("None");
            }
        });

        let text_style = egui::TextStyle::Heading;
        let row_height = ui.text_style_height(&text_style);
        let total_rows = run.frames.len() + 1;

        ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            row_height,
            total_rows,
            |ui, row_range| {
                Grid::new("Log Grid")
                    .striped(true)
                    .start_row(row_range.start)
                    .show(ui, |ui| {
                        for row in row_range {
                            if row == 0 {
                                // Header
                                ui.heading(format!("Frame"));
                                ui.add(Separator::default().vertical());
                                for player in &run.players {
                                    ui.horizontal(|ui| {
                                        ui.heading("Player ");
                                        ui.heading(run.player_label(*player));
                                    });
                                    ui.add(Separator::default().vertical());
                                }
                                ui.heading(format!("Status"));
                                ui.end_row();
                                continue;
                            }

                            let frame = row - 1;

                            let Some(frame_entries) = run.frames.get(&(frame as u64)).cloned()
                            else {
                                continue;
                            };
                            ui.heading(frame.to_string());
                            ui.add(Separator::default().vertical());

                            for logger in run.players.clone().iter() {
                                let player_number = run.player_number(*logger);
                                if let Some(player_entries) =
                                    frame_entries.player_entries.get(logger)
                                {
                                    ui.horizontal(|ui| {
                                        if let Some(SentInput { input, .. }) =
                                            &player_entries.sent_input
                                        {
                                            let color = if input == &Default::default() {
                                                Color32::GRAY
                                            } else {
                                                Color32::WHITE
                                            };

                                            ui.window_button(
                                                &player_entries.sent_input,
                                                false,
                                                RichText::new(fill::ENVELOPE).color(color),
                                                format!("P{} Sent {}", player_number, frame),
                                                |ui| {
                                                    ui.vertical(|ui| {
                                                        ui.heading("Sent Input");
                                                        ui.label(format!(
                                                            "Movement: {:?}",
                                                            input.movement
                                                        ));
                                                        ui.label(format!("Look: {:?}", input.look));
                                                        ui.label(format!("Jump: {:?}", input.jump));
                                                        ui.label(format!(
                                                            "Punch: {:?}",
                                                            input.punch
                                                        ));
                                                        if input.events.is_empty() {
                                                            ui.label("Events: []");
                                                        } else {
                                                            ui.label("Events: [");
                                                            for event in &input.events {
                                                                ui.label(format!(
                                                                    "    {:?}",
                                                                    event
                                                                ));
                                                            }
                                                            ui.label("]");
                                                        }
                                                    });
                                                },
                                            );
                                        }
                                        if !player_entries.received_inputs.is_empty() {
                                            ui.window_button(
                                                &player_entries.received_inputs,
                                                false,
                                                format!(
                                                    "{}{}",
                                                    player_entries.received_inputs.len(),
                                                    fill::ENVELOPE_OPEN
                                                ),
                                                format!(
                                                    "P{} Received Inputs Frame {}",
                                                    player_number, frame
                                                ),
                                                |ui| {
                                                    ui.vertical(|ui| {
                                                        ui.heading("Received Inputs");
                                                        for received_input in
                                                            &player_entries.received_inputs
                                                        {
                                                            ui.horizontal(|ui| {
                                                                ui.label(format!(
                                                                    "Frame {} from",
                                                                    received_input.sent_input.frame
                                                                ));
                                                                ui.label(
                                                                    run.player_label(
                                                                        received_input
                                                                            .sent_input
                                                                            .sender,
                                                                    ),
                                                                );
                                                            });
                                                        }
                                                    });
                                                },
                                            );
                                        }

                                        let mut start = frame;
                                        if let Some(rollback) = &player_entries.rollback {
                                            start = rollback.rolled_back_to as usize;
                                        }

                                        if player_entries.dropped_frame.is_some() {
                                            ui.label(
                                                RichText::new(fill::HAND_PALM).color(Color32::RED),
                                            );
                                        } else {
                                            let frames = if start == frame {
                                                frame.to_string()
                                            } else {
                                                format!("{}-{}", start, frame)
                                            };

                                            let highlighted = player_entries
                                                .contains_state(&run.highlighted_state);
                                            ui.window_button(
                                                &player_entries.dropped_frame,
                                                highlighted,
                                                format!("{}{}", fill::CPU, frames),
                                                format!("P{} Frame {}", player_number, frame),
                                                |ui| {
                                                    ui.vertical(|ui| {
                                                        for (frame, states) in &player_entries
                                                            .frame_states
                                                            .iter()
                                                            .group_by(|state| state.frame)
                                                        {
                                                            ui.heading(format!("Frame {}", frame));
                                                            for state in states {
                                                                run.state_label(ui, state);
                                                            }
                                                        }
                                                    });
                                                },
                                            );
                                        }

                                        if !player_entries.spawned_nodes_alive.is_empty() {
                                            ui.window_button(
                                                &(
                                                    frame,
                                                    player_entries
                                                        .spawned_nodes_alive
                                                        .get(&(frame as u64)),
                                                ),
                                                false,
                                                format!(
                                                    "{}{}",
                                                    fill::HEARTBEAT,
                                                    player_entries
                                                        .spawned_nodes_alive
                                                        .get(&(frame as u64))
                                                        .map(|nodes| nodes.len())
                                                        .unwrap_or_default()
                                                ),
                                                format!("P{} Tracked Nodes", player_number),
                                                |ui| {
                                                    ui.vertical(|ui| {
                                                        for frame in start..=frame {
                                                            ui.heading(format!("Frame {}", frame));
                                                            let spawned_nodes = player_entries
                                                                .spawned_nodes_alive
                                                                .get(&(frame as u64))
                                                                .cloned()
                                                                .unwrap_or_else(|| Vec::new());
                                                            for node in spawned_nodes {
                                                                ui.label(node.node_path.clone());
                                                            }
                                                        }
                                                    });
                                                },
                                            );
                                        }

                                        if !player_entries.events.is_empty() {
                                            let event_count = player_entries
                                                .events
                                                .values()
                                                .map(|events| events.len())
                                                .sum::<usize>();
                                            ui.window_button(
                                                &(frame, "events"),
                                                false,
                                                format!("{}{}", fill::ARTICLE, event_count,),
                                                format!("P{} Events", player_number),
                                                |ui| {
                                                    ui.vertical(|ui| {
                                                        for frame in start..=frame {
                                                            let events = player_entries
                                                                .events
                                                                .get(&(frame as u64))
                                                                .cloned()
                                                                .unwrap_or_else(|| BTreeSet::new());
                                                            if !events.is_empty() {
                                                                ui.heading(format!(
                                                                    "Frame {}",
                                                                    frame
                                                                ));
                                                                for node in events {
                                                                    ui.label(format!(
                                                                        "{}: {}",
                                                                        node.event.clone(),
                                                                        node.data.clone()
                                                                    ));
                                                                }
                                                            }
                                                        }
                                                    });
                                                },
                                            );
                                        }
                                    });
                                }
                                ui.add(Separator::default().vertical());
                            }

                            let highlighted = frame_entries
                                .sync_state
                                .contains_state(&run.highlighted_state);
                            match &frame_entries.sync_state {
                                SyncState::Synced { consensus } => {
                                    ui.window_button(
                                        &(frame, "sync state"),
                                        highlighted,
                                        RichText::new(fill::CHECK_FAT).color(Color32::GREEN),
                                        format!("Frame {} Consensus State", frame),
                                        |ui| {
                                            ui.vertical(|ui| {
                                                for state in consensus {
                                                    run.state_label(ui, state);
                                                }
                                            });
                                        },
                                    );
                                }
                                SyncState::Desynced { disagreements } => {
                                    ui.window_button(
                                        &(frame, "sync state"),
                                        highlighted,
                                        RichText::new(fill::X).color(Color32::RED),
                                        format!("Frame {} State Disagreements", frame),
                                        |ui| {
                                            ui.vertical(|ui| {
                                                for ((path, key), values) in disagreements {
                                                    let path = trim_path(path);
                                                    ui.heading(format!("{path}::{key}"));

                                                    for argument in values {
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                run.player_label(argument.player),
                                                            );
                                                            if let Some(state) = &argument.state {
                                                                run.state_value_label(ui, state);
                                                            } else {
                                                                ui.label("None");
                                                            }
                                                        });
                                                    }
                                                }
                                            });
                                        },
                                    );
                                }
                            }

                            ui.end_row();
                        }
                    });
            },
        );
    });
}
