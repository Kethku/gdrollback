use std::{fs::DirEntry, path::Path, time::SystemTime};

use anyhow::{anyhow, Result};
use indoc::indoc;
use rusqlite::{named_params, params, Connection};
use uuid::Uuid;

use crate::message::SentInput;

use super::{FrameState, LogEntry, RunInfo};

pub struct LogReader {
    pub run: Uuid,
    connection: Connection,
}

impl LogReader {
    pub fn list_runs() -> Result<Vec<(SystemTime, Uuid)>> {
        let directory = super::log_file_directory()?;
        let mut runs = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if let Ok((time, run)) = Self::parse_log_metadata(entry) {
                let mut found = false;
                for (existing_time, existing_run) in &mut runs {
                    if *existing_run == run {
                        *existing_time = std::cmp::min(*existing_time, time);
                        found = true;
                        break;
                    }
                }
                if !found {
                    runs.push((time, run));
                }
            }
        }
        runs.sort_by_key(|(time, _)| *time);
        Ok(runs)
    }

    pub fn parse_log_metadata(entry: DirEntry) -> Result<(SystemTime, Uuid)> {
        let time = entry.metadata()?.created()?;
        let file_name = entry.file_name();
        let file_name = file_name
            .to_str()
            .ok_or(anyhow!("File name not a standard string"))?
            .trim_end_matches(".db");
        let run = Self::parse_log_run_id(file_name)?;
        Ok((time, run))
    }

    pub fn parse_log_run_id(log_path: &str) -> Result<Uuid> {
        let file_name = Path::new(log_path)
            .file_name()
            .ok_or(anyhow!("log path does not have a file name"))?
            .to_str()
            .ok_or(anyhow!("log path not a standard string"))?;
        Ok(Uuid::parse_str(
            file_name
                .split('_')
                .next()
                .ok_or(anyhow!("Log file name incorrect"))?,
        )?)
    }

    pub fn delete_run(run: Uuid) -> Result<()> {
        let log_directory = super::log_file_directory()?;
        // Delete all files in the log directory containing the run id in their name
        for entry in std::fs::read_dir(log_directory)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .ok_or(anyhow!("File name not a standard string"))?
                .to_str()
                .ok_or(anyhow!("File name not a standard string"))?
                .contains(run.to_string().as_str())
            {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    pub fn load_run(run: Uuid) -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        super::setup_connection(&connection)?;

        let directory = super::log_file_directory()?;
        let run_string = run.to_string();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .ok_or(anyhow!("File name not a standard string"))?
                .starts_with(&run_string)
            {
                let file_path = entry
                    .path()
                    .to_str()
                    .ok_or(anyhow!("File path not a standard string"))?
                    .to_string();
                let mut sql = format!("ATTACH DATABASE '{file_path}' AS run;\n");
                for table in LogEntry::table_names() {
                    sql.push_str(&format!("INSERT INTO {table} SELECT * FROM run.{table};\n"));
                }
                sql.push_str("DETACH DATABASE run;");
                connection.execute_batch(&sql)?;
            }
        }

        Ok(Self { run, connection })
    }

    pub fn load_log_file(file_path: &str) -> Result<Self> {
        let run = Self::parse_log_run_id(file_path)?;
        let connection = Connection::open(file_path)?;
        Ok(Self { run, connection })
    }

    pub fn players(&self) -> Result<Vec<Uuid>> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT DISTINCT sender FROM sent_inputs")?;

        let players = statement.query_and_then([], |row| {
            let uuid = row.get::<_, Vec<u8>>(0)?;
            Ok(Uuid::from_slice(&uuid)?)
        })?;

        players.collect()
    }

    pub fn frame_count(&self) -> Result<u64> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT MAX(frame) FROM sent_inputs")?;

        Ok(statement.query_row([], |row| row.get::<_, u64>(0))?)
    }

    /// Finds the last frame where the given player rolledback past the given frame
    pub fn last_update_for_frame(&self, player: Uuid, frame: u64) -> Result<u64> {
        let mut statement = self.connection.prepare_cached(indoc! {"
                SELECT MAX(latest_frame)
                FROM (SELECT latest_frame, player, frame
                      FROM frame_states
                      WHERE player = ? AND frame = ?)
            "})?;

        Ok(
            statement.query_row(params![player.as_bytes(), &frame], |row| {
                row.get::<_, u64>(0)
            })?,
        )
    }

    pub fn latest_states_for_frame(&self, player: Uuid, frame: u64) -> Result<Vec<FrameState>> {
        let last_update_frame = self.last_update_for_frame(player, frame)?;
        let mut statement = self
            .connection
            .prepare_cached(indoc! {"
                SELECT path, key, value_text, value_hash 
                FROM frame_states 
                WHERE player = ? AND frame = ? AND latest_frame = ?
            "})
            .unwrap();
        let mut rows = statement.query(params![player.as_bytes(), &frame, &last_update_frame])?;

        let mut states = Vec::new();
        while let Some(row) = rows.next()? {
            let path = row.get::<_, String>(0)?;
            let key = row.get::<_, String>(1)?;
            let value_text = row.get::<_, String>(2)?;
            let value_hash_bytes: [u8; 8] = row.get::<_, Vec<u8>>(3)?.try_into().unwrap();
            let value_hash = u64::from_be_bytes(value_hash_bytes);
            states.push(FrameState {
                frame,
                latest_frame: last_update_frame,
                player,
                path,
                key,
                value_text,
                value_hash,
            });
        }

        Ok(states)
    }

    pub fn run_infos(&self) -> Result<Vec<RunInfo>> {
        RunInfo::read(&self.connection)
    }

    pub fn received_inputs_for_tick(&self, tick: u64) -> Result<Vec<SentInput>> {
        let mut statement = self.connection.prepare_cached(indoc! {"
                SELECT sent_input
                FROM received_inputs
                WHERE received_frame = :tick
            "})?;

        let inputs = statement.query_and_then(
            named_params! {
                ":tick": tick,
            },
            |row| {
                let sent_input = bincode::deserialize::<SentInput>(&row.get::<_, Vec<u8>>(0)?)?;
                Ok(sent_input)
            },
        )?;

        inputs.collect()
    }

    pub fn sent_input_for_tick(&self, tick: u64) -> Result<Vec<u8>> {
        let mut statement = self.connection.prepare_cached(indoc! {"
            SELECT input
            FROM sent_inputs
            WHERE frame = :tick
        "})?;

        Ok(statement.query_row(
            named_params! {
                ":tick": tick,
            },
            |row| row.get::<_, Vec<u8>>(0),
        )?)
    }

    pub fn log_entries(&self) -> Result<Vec<LogEntry>> {
        LogEntry::read(&self.connection)
    }
}
