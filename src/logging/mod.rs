mod log_entry;
mod log_reader;
mod log_writer;

use anyhow::Result;
use godot::engine::ProjectSettings;
use indoc::indoc;
use rusqlite::Connection;
use std::path::PathBuf;

pub use log_entry::*;
pub use log_reader::*;
pub use log_writer::*;

pub fn log_file_directory() -> Result<PathBuf> {
    let project_settings = ProjectSettings::singleton();
    let directory_string: String = project_settings.globalize_path("logs".into()).into();
    let directory_path = PathBuf::from(directory_string);
    std::fs::create_dir_all(&directory_path)?;
    Ok(directory_path.to_owned())
}

pub fn setup_connection(connection: &Connection) -> Result<()> {
    connection.execute_batch(indoc! {"
            PRAGMA journal_mode=WAL2;
            PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON;
            PRAGMA busy_timeout=100;
        "})?;

    LogEntry::setup_tables(connection)?;

    Ok(())
}
