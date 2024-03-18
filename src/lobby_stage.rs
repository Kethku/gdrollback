use std::collections::HashMap;

use anyhow::Result;
use godot::prelude::*;
use udp_ext::persistent::PersistentSocketSender;
use uuid::Uuid;

use crate::{
    message::Message, play_stage::PlayStage, sync_manager::RollbackSyncManager,
    sync_stage::SyncStage, Context,
};

const SCHEDULE_TICKS: u32 = 1 * 60;

pub struct LobbyStage {
    ready: bool,
    scheduled_start: Option<u32>,
    early_inputs: Vec<Message>,
    peers_ready: HashMap<Uuid, bool>,
}

impl LobbyStage {
    pub fn new() -> Self {
        Self {
            ready: false,
            scheduled_start: None,
            early_inputs: Vec::new(),
            peers_ready: HashMap::new(),
        }
    }

    pub fn tick(&mut self, node: &mut Gd<Node>, cx: &Context) -> Result<Option<SyncStage>> {
        if let Some(ticks_till_start) = self.scheduled_start.as_mut() {
            if *ticks_till_start == 0 {
                self.scheduled_start = None;
                let node = (*node).clone();
                let mut this = node.cast::<RollbackSyncManager>();
                this.call_deferred("start_game".into(), &[]);
                return Ok(Some(SyncStage::Play(PlayStage::new(
                    self.early_inputs.clone(),
                    cx,
                ))));
            }

            *ticks_till_start -= 1;
        }
        Ok(None)
    }

    pub fn handle_message(
        &mut self,
        node: &mut Gd<Node>,
        message: Message,
        sender: PersistentSocketSender<Uuid>,
        cx: &mut Context,
    ) -> Result<()> {
        match message {
            Message::Connect(id) => {
                // if uuid is not in peers, add it, send a connect in reply and gossip the address to all
                // other peers. Also gossip to the newly connected peer all the peers you are
                // connected to
                let PersistentSocketSender::Unconnected(address) = sender else {
                    return Ok(());
                };

                node.emit_signal("connected".into(), &[Variant::from(id.to_string())]);

                cx.send_to_address(address, Message::Connect(cx.local_id()))?;

                cx.broadcast(Message::GossipPeer(id, address.to_string()))?;
                self.update_ready(node, false, cx)?;
                for peer in cx.peers() {
                    let peer_address = cx.address(peer).unwrap();
                    cx.send_to_address(
                        address,
                        Message::GossipPeer(peer, peer_address.to_string()),
                    )?;
                }
                cx.connect(id, address);
            }
            Message::GossipPeer(gossiped_id, gossiped_address) => {
                if cx.address(gossiped_id).is_some() || gossiped_id == cx.local_id() {
                    return Ok(());
                }

                cx.send_to_address(gossiped_address, Message::Connect(cx.local_id()))?;
            }
            Message::UpdateReady(ready) => {
                // Mark the peer with the value. If all peers are ready, and your
                // id is lowest, send a schedule start message to all peers
                let PersistentSocketSender::Connected(id) = sender else {
                    panic!("UpdateReady message from unconnected sender");
                };
                self.peers_ready.insert(id, ready);
                dbg!(id);
                self.try_schedule_start(node, cx)?;
            }
            Message::ScheduleStart(run) => {
                let PersistentSocketSender::Connected(id) = sender else {
                    panic!("ScheduleStart message from unconnected sender");
                };

                let start_adjustment = (cx.average_response_time(id).unwrap() / 2).as_millis() / 16;
                godot_print!("Start adjustment: {}", start_adjustment);
                self.scheduled_start = Some(SCHEDULE_TICKS.saturating_sub(start_adjustment as u32));
                cx.set_run(run).expect("Could not set run on logger");
                godot_print!("Scheduled start");
                node.emit_signal("start_scheduled".into(), &[]);
            }
            message @ Message::Input { .. } => {
                self.early_inputs.push(message);
            }
            _ => {}
        }

        Ok(())
    }

    pub fn update_ready(
        &mut self,
        node: &mut Gd<Node>,
        value: bool,
        cx: &mut Context,
    ) -> Result<()> {
        self.ready = value;
        cx.broadcast(Message::UpdateReady(self.ready))?;
        self.try_schedule_start(node, cx)?;

        Ok(())
    }

    pub fn try_schedule_start(&mut self, node: &mut Gd<Node>, cx: &mut Context) -> Result<()> {
        if self.ready
            && cx
                .peers()
                .iter()
                .all(|peer| self.peers_ready.get(peer).copied().unwrap_or_default())
        {
            let lowest_id = cx
                .peers()
                .into_iter()
                .chain(std::iter::once(cx.local_id()))
                .min()
                .expect("Could not find lowest id");
            if lowest_id == cx.local_id() {
                let run = Uuid::new_v4();
                cx.set_run(run).expect("Could not set run on logger");
                cx.broadcast(Message::ScheduleStart(run))?;

                let average_lobby_response_millis = cx.average_lobby_response_time().as_millis();
                let start_adjustment = if average_lobby_response_millis > 0 {
                    average_lobby_response_millis / 32
                } else {
                    0
                };
                godot_print!("Start adjustment: {}", start_adjustment);
                self.scheduled_start = Some(SCHEDULE_TICKS + start_adjustment as u32);
                godot_print!("Broadcast scheduled start");
                node.emit_signal("start_scheduled".into(), &[]);
            }
        }

        Ok(())
    }
}
