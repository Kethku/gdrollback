use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{channel, Sender},
        Arc,
    },
    thread,
};

use anyhow::Result;
use rusqlite::Connection;
use uuid::Uuid;

use crate::{message::SentInput, Context};

use super::{
    log_file_directory, setup_connection, DroppedFrame, Event, FrameState, LogEntry, ReceivedInput,
    Rollback, RunInfo, SpawnedNodeAlive,
};

pub struct LogWriter {
    run_sender: Sender<(Uuid, Uuid)>,
    log_sender: Sender<LogEntry>,
    id_counter: AtomicUsize,
    enabled: Arc<AtomicBool>,
}

impl LogWriter {
    pub fn new() -> Self {
        let (run_sender, run_receiver) = channel::<(Uuid, Uuid)>();
        let (log_sender, log_receiver) = channel::<LogEntry>();
        let enabled = Arc::new(AtomicBool::new(true));
        let directory = log_file_directory().unwrap();

        thread::spawn({
            let enabled = enabled.clone();
            move || {
                let (run, id) = run_receiver.recv().expect("Failed to receive run id");

                let file_path = directory.join(format!("{run}_{}.db", id.to_string()));

                let mut connection = Connection::open(file_path).unwrap();
                setup_connection(&connection).unwrap();

                while let Ok(entry) = log_receiver.recv() {
                    let mut entries = vec![entry];
                    while let Ok(entry) = log_receiver.try_recv() {
                        entries.push(entry);
                    }

                    if !enabled.load(Ordering::SeqCst) {
                        continue;
                    }

                    let transaction = connection.transaction().unwrap();
                    for entry in entries {
                        entry
                            .write(&transaction)
                            .expect(&format!("Failed to write {entry:?} to database"));
                    }
                    transaction
                        .commit()
                        .expect("Failed to commit transaction to db");
                }
            }
        });

        Self {
            run_sender,
            log_sender,
            id_counter: AtomicUsize::new(0),
            enabled,
        }
    }

    pub fn set_run(&self, run: Uuid, id: Uuid) -> Result<()> {
        self.run_sender.send((run, id))?;
        Ok(())
    }

    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    pub fn run_info(&self, cx: &Context) -> Result<()> {
        self.log_sender.send(LogEntry::RunInfo(RunInfo {
            local_id: cx.local_id(),
            peers: cx.peers(),
        }))?;
        Ok(())
    }

    pub fn sent_input(&self, sent_input: SentInput) -> Result<()> {
        self.log_sender.send(LogEntry::SentInput(sent_input))?;
        Ok(())
    }

    pub fn received_input(
        &self,
        received_frame: u64,
        sent_input: SentInput,
        cx: &Context,
    ) -> Result<()> {
        self.log_sender
            .send(LogEntry::ReceivedInput(ReceivedInput {
                received_frame,
                receiver: cx.local_id(),
                sent_input,
            }))?;
        Ok(())
    }

    pub fn received_input_manual(
        &self,
        received_frame: u64,
        receiver: Uuid,
        sent_input: SentInput,
    ) -> Result<()> {
        self.log_sender
            .send(LogEntry::ReceivedInput(ReceivedInput {
                received_frame,
                receiver,
                sent_input,
            }))?;
        Ok(())
    }

    pub fn dropped_frame(
        &self,
        frame: u64,
        frame_missing_input: u64,
        lagger: Uuid,
        cx: &Context,
    ) -> Result<()> {
        self.log_sender.send(LogEntry::DroppedFrame(DroppedFrame {
            id: self.id_counter.fetch_add(1, Ordering::SeqCst),
            frame,
            frame_missing_input,
            lagger,
            dropper: cx.local_id(),
        }))?;

        Ok(())
    }

    pub fn rollback(&self, frame: u64, rolled_back_to: u64, cx: &Context) -> Result<()> {
        self.log_sender.send(LogEntry::Rollback(Rollback {
            frame,
            rolled_back_to,
            updater: cx.local_id(),
        }))?;

        Ok(())
    }

    pub fn state(
        &self,
        path: String,
        key: String,
        value_text: String,
        value_hash: u64,
        cx: &Context,
    ) -> Result<()> {
        self.log_sender.send(LogEntry::FrameState(FrameState {
            frame: cx.current_tick(),
            latest_frame: cx.latest_tick(),
            player: cx.local_id(),
            path,
            key,
            value_text,
            value_hash,
        }))?;

        Ok(())
    }

    pub fn spawned_node_alive(&self, node_path: String, cx: &Context) -> Result<()> {
        self.log_sender
            .send(LogEntry::SpawnedNodeAlive(SpawnedNodeAlive {
                frame: cx.current_tick(),
                latest_frame: cx.latest_tick(),
                player: cx.local_id(),
                node_path,
            }))?;

        Ok(())
    }

    pub fn event(&self, event: String, data: String, cx: &Context) -> Result<()> {
        self.event_for_frame(cx.current_tick(), event, data, cx)
    }

    pub fn event_for_frame(
        &self,
        frame: u64,
        event: String,
        data: String,
        cx: &Context,
    ) -> Result<()> {
        self.log_sender.send(LogEntry::Event(Event {
            id: self.id_counter.fetch_add(1, Ordering::SeqCst),
            frame,
            latest_frame: cx.latest_tick(),
            player: cx.local_id(),
            event,
            data,
        }))?;

        Ok(())
    }
}
