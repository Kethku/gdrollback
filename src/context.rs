use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use anyhow::Result;
use uuid::Uuid;

use udp_ext::persistent::{PersistentEvent, PersistentSocket, PersistentSocketSender};

use crate::{
    logging::{LogWriter, RunInfo},
    message::Message,
};

pub struct Context {
    local_id: Uuid,
    current_tick: u64,
    latest_tick: u64,
    logger: LogWriter,
    socket: PersistentSocket<Uuid>,

    replay_overrides: Option<RunInfo>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            local_id: Uuid::new_v4(),
            current_tick: 0,
            latest_tick: 0,
            logger: LogWriter::new(),
            socket: PersistentSocket::bind(0).expect("Could not bind random port"),

            replay_overrides: None,
        }
    }

    pub fn set_replay(&mut self, overrides: RunInfo) {
        self.logger.disable();
        self.replay_overrides = Some(overrides);
    }

    pub fn clear_replay(&mut self) {
        self.logger.enable();
        self.replay_overrides = None;
    }

    pub fn local_id(&self) -> Uuid {
        self.replay_overrides
            .as_ref()
            .map(|overrides| overrides.local_id.clone())
            .unwrap_or_else(|| self.local_id)
    }

    pub fn peers(&self) -> Vec<Uuid> {
        self.replay_overrides
            .as_ref()
            .map(|overrides| overrides.peers.clone())
            .unwrap_or_else(|| self.socket.peers())
    }

    /// The leader is the peer with the lowest Uuid in the group. This is an arbitrary
    /// decision based on the
    pub fn is_leader(&self) -> bool {
        self.peers()
            .into_iter()
            .min()
            .map(|min_peer| min_peer.cmp(&self.local_id) == std::cmp::Ordering::Greater)
            .unwrap_or(true)
    }

    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    pub fn latest_tick(&self) -> u64 {
        self.latest_tick
    }

    pub fn increment_latest_tick(&mut self) -> u64 {
        self.latest_tick += 1;
        self.latest_tick
    }

    pub fn set_current_tick(&mut self, tick: u64) {
        self.current_tick = tick;
    }

    pub fn set_run(&self, run: Uuid) -> Result<()> {
        if self.replay_overrides.is_some() {
            panic!("Can't set run during a replay");
        }

        self.logger.set_run(run, self.local_id)
    }

    pub fn address(&self, peer: Uuid) -> Option<SocketAddr> {
        if self.replay_overrides.is_some() {
            panic!("Can't fetch address during a replay");
        }

        self.socket.address(peer)
    }

    pub fn send_to(&mut self, peer: Uuid, message: Message) -> Result<()> {
        if self.replay_overrides.is_none() {
            self.socket.send_to(peer, message)?;
        }
        Ok(())
    }

    pub fn send_to_address(&mut self, address: impl ToSocketAddrs, message: Message) -> Result<()> {
        if self.replay_overrides.is_none() {
            self.socket.send_to_address(address, message)?;
        }
        Ok(())
    }

    pub fn connect(&mut self, peer: Uuid, address: SocketAddr) {
        if self.replay_overrides.is_some() {
            panic!("Can't connect during a replay");
        }

        self.socket.connect(peer, address)
    }

    pub fn broadcast(&mut self, message: Message) -> Result<()> {
        if self.replay_overrides.is_none() {
            self.socket.broadcast(message)?;
        }
        Ok(())
    }

    pub fn average_lobby_response_time(&self) -> Duration {
        if self.replay_overrides.is_some() {
            panic!("Can't call average_lobby_response_time during a replay");
        }
        self.socket.average_lobby_response_time()
    }

    pub fn average_response_time(&self, peer: Uuid) -> Option<Duration> {
        if self.replay_overrides.is_some() {
            panic!("Can't call average_response_time during a replay");
        }

        self.socket.average_response_time(peer)
    }

    pub fn pump_socket(&mut self) -> Result<Vec<(PersistentEvent, PersistentSocketSender<Uuid>)>> {
        self.socket.pump()
    }

    pub fn set_port(&mut self, port: u16) -> Result<()> {
        if self.replay_overrides.is_some() {
            panic!("Can't set port  during a replay");
        }

        if self.socket.local_addr()?.port() == port as u16 {
            return Ok(());
        }

        self.socket = PersistentSocket::bind(port as u16)?;

        Ok(())
    }

    pub fn logger(&self) -> &LogWriter {
        &self.logger
    }
}
