use anyhow::Result;
use godot::prelude::*;
use udp_ext::persistent::PersistentSocketSender;
use uuid::Uuid;

use crate::{
    lobby_stage::LobbyStage,
    message::Message,
    play_stage::{Input, PlayStage},
    replay_stage::ReplayStage,
    Context,
};

pub enum SyncStage {
    Lobby(LobbyStage),
    Play(PlayStage),
    Replay(ReplayStage),
}

impl SyncStage {
    pub fn tick(&mut self, node: &mut Gd<Node>, cx: &mut Context) -> Result<()> {
        let next_stage = match self {
            SyncStage::Lobby(lobby_stage) => lobby_stage.tick(node, cx)?,
            SyncStage::Play(play_stage) => play_stage.tick(node, cx)?,
            SyncStage::Replay(replay_stage) => replay_stage.tick(node, cx)?,
        };

        if let Some(next_stage) = next_stage {
            *self = next_stage
        }

        Ok(())
    }

    pub fn handle_message(
        &mut self,
        node: &mut Gd<Node>,
        message: Message,
        address: PersistentSocketSender<Uuid>,
        cx: &mut Context,
    ) -> Result<()> {
        match self {
            SyncStage::Lobby(lobby_stage) => lobby_stage.handle_message(node, message, address, cx),
            SyncStage::Play(play_stage) => play_stage.handle_message(message, cx),
            SyncStage::Replay(_) => {
                // Noop. During a replay messages are thrown out.
                Ok(())
            }
        }
    }

    pub fn input(&self, id: String, cx: &Context) -> Input {
        match self {
            SyncStage::Lobby(_) => Input::default(),
            SyncStage::Play(play_stage) => play_stage.input(id, cx),
            SyncStage::Replay(replay_stage) => replay_stage.input(id, cx),
        }
    }

    pub fn advantage(&self) -> f64 {
        match self {
            SyncStage::Lobby(_) => 0.0,
            SyncStage::Play(play_stage) => play_stage.advantage(),
            SyncStage::Replay(replay_stage) => replay_stage.advantage(),
        }
    }
}
