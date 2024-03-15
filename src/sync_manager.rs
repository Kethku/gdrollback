use godot::prelude::*;
use itertools::Itertools;
use udp_ext::persistent::PersistentEvent;

use crate::{
    lobby_stage::LobbyStage, logging::LogReader, message::Message, play_stage::PlayStage,
    replay_stage::ReplayStage, sync_stage::SyncStage, Context,
};

#[derive(GodotClass)]
#[class(base = Node)]
pub struct RollbackSyncManager {
    pub context: Context,

    pub stage: SyncStage,

    pub node: Base<Node>,
}

#[godot_api]
impl INode for RollbackSyncManager {
    fn init(node: Base<Node>) -> Self {
        Self {
            context: Context::new(),

            stage: SyncStage::Lobby(LobbyStage::new()),

            node,
        }
    }

    fn ready(&mut self) {
        let mut node = self.base_mut();
        node.set_process(true);
        node.set_physics_process(true);
    }

    fn physics_process(&mut self, _: f64) {
        let socket_results = self.context.pump_socket().expect("Couldn't pump socket");

        let messages = socket_results.into_iter().filter_map(|(message, address)| {
            if let PersistentEvent::FrameCompleted(_, mut message) = message {
                Some((message.read_serializable()?, address))
            } else {
                None
            }
        });

        for (message, address) in messages {
            self.stage
                .handle_message(&mut self.node.to_gd(), message, address, &mut self.context)
                .expect("Couldn't handle message");
        }

        self.stage
            .tick(&mut self.node.to_gd(), &mut self.context)
            .expect("Could not tick stage");
    }
}

#[godot_api]
impl RollbackSyncManager {
    // SIGNALS

    #[signal]
    fn start_scheduled();
    #[signal]
    fn connected(id: String);
    #[signal]
    fn started();

    // LOBBY APIS

    #[func]
    pub fn update_ready(&mut self, value: bool) {
        if let SyncStage::Lobby(lobby) = &mut self.stage {
            lobby
                .update_ready(&mut self.node.to_gd(), value, &mut self.context)
                .expect("Couldn't update ready");
        }
    }

    #[func(gd_self)]
    pub fn replay(mut this: Gd<Self>, replay_path: String) {
        let log_reader = LogReader::load_log_file(&replay_path).expect("Could not load log file");
        {
            let mut this = this.bind_mut();
            let stage = SyncStage::Replay(
                ReplayStage::new(log_reader, &mut this.context)
                    .expect("Could not create replay stage"),
            );
            this.stage = stage;
        }
        godot_print!("Started replay from log file");
        this.emit_signal("started".into(), &[]);
    }

    #[func]
    fn host(&mut self, port: u16) {
        godot_print!("Hosting on port {}", port);
        self.context.set_port(port).expect("Could not set port");
    }

    #[func]
    fn join(&mut self, ip: String, port: u32) {
        godot_print!("Connecting to {}:{}", ip, port);
        self.context
            .send_to_address(
                format!("{}:{}", ip, port),
                Message::Connect(self.context.local_id()),
            )
            .expect("Could not send message");
    }

    #[func(gd_self)]
    fn start_game(mut this: Gd<Self>) {
        {
            let this = this.bind();
            godot_print!("Started with {} peers", this.context.peers().len());
            this.context
                .logger()
                .run_info(&this.context)
                .expect("Could not log run info");
        }
        this.emit_signal("started".into(), &[]);
    }

    // PLAYING APIS

    #[func]
    pub fn local_id(&mut self) -> String {
        self.context.local_id().to_string()
    }

    #[func]
    pub fn remote_ids(&mut self) -> Array<Variant> {
        self.context
            .peers()
            .into_iter()
            .map(|id| Variant::from(id.to_string()))
            .collect()
    }

    #[func]
    pub fn ids(&mut self) -> Array<Variant> {
        self.context
            .peers()
            .into_iter()
            .chain(std::iter::once(self.context.local_id()))
            .sorted()
            .map(|id| Variant::from(id.to_string()))
            .collect()
    }

    #[func]
    pub fn is_leader(&mut self) -> bool {
        self.context.is_leader()
    }

    #[func]
    pub fn input(&mut self, id: String) -> Variant {
        self.stage.input(id, &self.context)
    }

    #[func]
    pub fn advantage(&mut self) -> f64 {
        self.stage.advantage()
    }

    #[func(gd_self)]
    fn execute_tick(this: Gd<Self>) {
        PlayStage::execute_tick(this);
    }

    #[func(gd_self)]
    fn despawn(this: Gd<Self>, node: Gd<Node>) {
        PlayStage::despawn(this, &node);
    }

    #[func(gd_self)]
    fn spawn(
        this: Gd<Self>,
        name: String,
        parent: Gd<Node>,
        scene: Gd<PackedScene>,
        data: Dictionary,
    ) -> Gd<Node> {
        let data = Variant::from(data);
        PlayStage::spawn(this, name, &parent, scene, data)
    }

    #[func]
    fn log(&mut self, event: String) {
        self.context
            .logger()
            .event("GODOT".to_string(), event, &self.context)
            .expect("Could not log event");
    }
}
