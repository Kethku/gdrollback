mod log_entry;
mod log_reader;
mod log_writer;

use anyhow::{anyhow, Result};
use indoc::indoc;
use rusqlite::Connection;
use std::path::PathBuf;

pub use log_entry::*;
pub use log_reader::*;
pub use log_writer::*;

pub fn log_file_directory() -> Result<PathBuf> {
    let directory = dirs::data_local_dir()
        .ok_or(anyhow!("Could not find local data dir"))?
        .join("UPCTP");
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

pub fn setup_connection(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(indoc! {"
            PRAGMA journal_mode=WAL2;
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;
            PRAGMA busy_timeout=100;
        "})?;

    LogEntry::setup_tables(connection)?;

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, thread};

    use rand::prelude::*;
    use uuid::Uuid;

    use crate::input::Input;

    use super::*;

    #[test]
    fn test_log() {
        const FRAMES: u32 = 50;
        let session_id = Uuid::new_v4();
        let players = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];

        let mut join_handles = HashMap::new();
        for player in players.clone() {
            join_handles.insert(
                player,
                thread::spawn({
                    let players = players.clone();
                    move || {
                        let mut rng = rand::thread_rng();
                        let log = LogWriter::new(player, session_id).unwrap();
                        for frame in 0..FRAMES {
                            log.log_sent_input(frame, player, Input::default()).unwrap();
                        }

                        for remote_player in players {
                            if remote_player != player {
                                let mut frame = 0;
                                let mut delay = 0;
                                while frame < FRAMES {
                                    if rng.gen::<f32>() < 0.1 {
                                        delay += 1;
                                    } else {
                                        frame += 1;
                                        log.log_received_input(
                                            frame,
                                            player,
                                            remote_player,
                                            frame + delay,
                                        )
                                        .unwrap();
                                    }
                                }
                            }
                        }
                    }
                }),
            );
        }

        for (_, join_handle) in join_handles.drain() {
            join_handle.join().unwrap()
        }

        let combined = LogReader::load_run(session_id).unwrap();

        let entries = combined.log_entries().unwrap();

        let sent_inputs = entries
            .iter()
            .filter_map(|entry| {
                if let LogEntry::SentInput(entry) = entry {
                    Some(entry)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let received_inputs = entries
            .iter()
            .filter_map(|entry| {
                if let LogEntry::ReceivedInput(entry) = entry {
                    Some(entry)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        LogReader::delete_run(session_id).unwrap();

        assert_eq!(sent_inputs.len(), players.len() * FRAMES as usize);

        assert_eq!(
            received_inputs.len(),
            sent_inputs.len() * (players.len() - 1)
        );
    }
}
