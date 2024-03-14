use anyhow::Result;
use godot::prelude::*;

use crate::{
    logging::LogReader,
    message::Message,
    play_stage::{Input, PlayStage},
    sync_stage::SyncStage,
    Context,
};

pub struct ReplayStage {
    log_reader: LogReader,
    pub play_stage: PlayStage,
}

impl ReplayStage {
    pub fn new(log_reader: LogReader, cx: &mut Context) -> Result<Self> {
        let run_info = log_reader.run_infos()?[0].clone();
        cx.set_replay(run_info);
        Ok(Self {
            log_reader,
            play_stage: PlayStage::new(Vec::new(), cx),
        })
    }

    pub fn tick(&mut self, node: &mut Gd<Node>, cx: &Context) -> Result<Option<SyncStage>> {
        let received_inputs = self
            .log_reader
            .received_inputs_for_tick(cx.latest_tick() + 1)?;
        for received_input in received_inputs {
            self.play_stage.handle_message(
                Message::Input {
                    sent_input: received_input,
                    last_received_frame: cx.latest_tick(),
                },
                cx,
            )?;
        }
        self.play_stage.tick(node, cx)?;
        Ok(None)
    }

    pub fn input(&self, id: String, cx: &Context) -> Input {
        self.play_stage.input(id, cx)
    }

    pub fn local_input(&self, cx: &Context) -> Input {
        self.log_reader
            .sent_input_for_tick(cx.latest_tick())
            .expect("Could not find sent input for tick")
    }

    pub fn advantage(&self) -> f64 {
        self.play_stage.advantage()
    }
}
