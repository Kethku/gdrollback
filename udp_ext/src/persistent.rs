use std::collections::{HashMap, VecDeque};
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use super::frame::*;
use super::messages::*;
use super::reliable::PacketId;

#[derive(Debug, PartialEq)]
pub enum PersistentEvent {
    PacketAcknowledged(PacketId),
    PacketResent(PacketId),
    FrameComponentRecieved(ComponentPosition),
    FrameCompleted(FrameId, IncomingMessage),
    FrameComponentSent(PacketId),
    PeerDisconnected,
}

#[derive(Debug, PartialEq)]
pub enum PersistentSocketSender<ID>
where
    ID: PartialEq + Eq + Hash + Clone + Copy,
{
    Connected(ID),
    Unconnected(SocketAddr),
}

impl<ID> Display for PersistentSocketSender<ID>
where
    ID: PartialEq + Eq + Hash + Clone + Copy + Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistentSocketSender::Connected(id) => write!(f, "Connected({})", id),
            PersistentSocketSender::Unconnected(addr) => write!(f, "Unconnected({})", addr),
        }
    }
}

/// Wrapper over frame sockets which tracks average reply times and disconnects.
pub struct PersistentSocket<ID>
where
    ID: PartialEq + Eq + Hash + Clone + Copy,
{
    frame: FrameSocket,
    sent_times: HashMap<(PacketId, SocketAddr), Instant>,
    ping_times: HashMap<ID, VecDeque<Duration>>,
    addresses_by_id: HashMap<ID, SocketAddr>,
    id_by_address: HashMap<SocketAddr, ID>,
}

impl<ID> PersistentSocket<ID>
where
    ID: PartialEq + Eq + Hash + Clone + Copy,
{
    pub const DISCONNECT_MILLIS: u64 = 5000;
    // TODO: Send persistent socket specific ping messages if the user
    // hasn't recieved a message from a peer in this amount of time
    pub const PING_MILLIS: u64 = 500;
    pub const PING_ROLLING_AVERAGE_SIZE: usize = 100;

    pub fn bind(port: u16) -> Result<PersistentSocket<ID>> {
        let frame = FrameSocket::bind(port)?;

        Ok(PersistentSocket {
            frame,
            sent_times: HashMap::new(),
            ping_times: HashMap::new(),
            addresses_by_id: HashMap::new(),
            id_by_address: HashMap::new(),
        })
    }

    pub fn send_to(&mut self, id: ID, message: impl IntoOutgoingMessage) -> Result<FrameId> {
        let remote_address = self
            .addresses_by_id
            .get(&id)
            .ok_or(anyhow!("No address found for this id"))?;
        let message = message.into();
        Ok(self.frame.send_to(message, remote_address)?)
    }

    pub fn send_to_address(
        &mut self,
        remote_address: impl ToSocketAddrs,
        message: impl IntoOutgoingMessage,
    ) -> Result<FrameId> {
        let remote_address = remote_address.to_socket_addrs()?.next().unwrap();
        let message = message.into();
        Ok(self.frame.send_to(message, remote_address)?)
    }

    pub fn broadcast(&mut self, message: impl IntoOutgoingMessage) -> Result<HashMap<ID, FrameId>> {
        let message = message.into();
        let mut results = HashMap::new();
        for (remote_address, id) in self.id_by_address.iter() {
            let frame_id = self.frame.send_to(message.clone(), remote_address)?;
            results.insert(*id, frame_id);
        }
        Ok(results)
    }

    pub fn connect(&mut self, id: ID, address: SocketAddr) {
        self.ping_times.insert(id.clone(), VecDeque::new());
        self.addresses_by_id.insert(id.clone(), address);
        self.id_by_address.insert(address, id);
    }

    pub fn peers(&self) -> Vec<ID> {
        self.addresses_by_id.keys().copied().collect()
    }

    pub fn address(&self, id: ID) -> Option<SocketAddr> {
        self.addresses_by_id.get(&id).cloned()
    }

    pub fn pump(&mut self) -> Result<Vec<(PersistentEvent, PersistentSocketSender<ID>)>> {
        let mut results = Vec::new();

        for (event, remote_address) in self.frame.pump()? {
            let sender = self.to_sender(remote_address);
            match event {
                FrameEvent::PacketAcknowledged(packet_id) => {
                    results.push((PersistentEvent::PacketAcknowledged(packet_id), sender));
                    self.record_acknowledgement(packet_id, remote_address);
                }
                FrameEvent::PacketResent(packet_id) => {
                    results.push((PersistentEvent::PacketResent(packet_id), sender));
                }
                FrameEvent::FrameComponentRecieved(component_position) => {
                    results.push((
                        PersistentEvent::FrameComponentRecieved(component_position),
                        sender,
                    ));
                }
                FrameEvent::FrameCompleted(frame_id, incoming_message) => {
                    results.push((
                        PersistentEvent::FrameCompleted(frame_id, incoming_message),
                        sender,
                    ));
                }
                FrameEvent::FrameComponentSent(packet_id) => {
                    results.push((PersistentEvent::FrameComponentSent(packet_id), sender));
                    self.record_send(packet_id, remote_address);
                }
            }
        }

        let mut disconnects = Vec::new();
        for ((ack_id, remote_address), sent_time) in self.sent_times.iter() {
            let sender = self.to_sender(*remote_address);
            if sent_time.elapsed()
                > Duration::from_millis(PersistentSocket::<ID>::DISCONNECT_MILLIS)
            {
                results.push((PersistentEvent::PeerDisconnected, sender));
                disconnects.push((*ack_id, *remote_address));
            }
        }
        for disconnect in disconnects {
            self.sent_times.remove(&disconnect);
        }

        Ok(results)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.frame.local_addr()
    }

    pub fn average_response_time(&self, id: ID) -> Option<Duration> {
        self.ping_times
            .get(&id)
            .filter(|times| !times.is_empty())
            .map(|times| times.iter().sum::<Duration>() / times.len() as u32)
    }

    pub fn average_lobby_response_time(&self) -> Duration {
        if self.ping_times.len() == 0 {
            Duration::from_secs(0)
        } else {
            self.ping_times.values().flatten().sum::<Duration>()
                / self.ping_times.values().flatten().count() as u32
        }
    }

    fn record_send(&mut self, packet_id: PacketId, remote_address: SocketAddr) {
        self.sent_times
            .insert((packet_id, remote_address), Instant::now());
    }

    fn record_acknowledgement(&mut self, packet_id: PacketId, remote_address: SocketAddr) {
        if let Some(sent_time) = self
            .sent_times
            .remove(&(packet_id, remote_address)) {
            if let Some(id) = self.id_by_address.get(&remote_address) {
                let ping_times = self.ping_times.get_mut(&id).unwrap();
                ping_times.push_front(sent_time.elapsed());
                if ping_times.len() > PersistentSocket::<ID>::PING_ROLLING_AVERAGE_SIZE {
                    ping_times.pop_back();
                }
            }
        }
    }

    fn to_sender(&self, remote_address: SocketAddr) -> PersistentSocketSender<ID> {
        if let Some(id) = self.id_by_address.get(&remote_address) {
            PersistentSocketSender::Connected(id.clone())
        } else {
            PersistentSocketSender::Unconnected(remote_address)
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        thread::{sleep, spawn},
        time::Duration,
    };

    use crate::{
        messages::OutgoingMessage,
        persistent::{PersistentEvent, PersistentSocket},
    };

    #[ignore]
    #[test]
    fn stress_test() {
        let mut persistent_1 = PersistentSocket::<usize>::bind(0).unwrap();
        let mut persistent_2 = PersistentSocket::<usize>::bind(0).unwrap();
        let address_2 = format!("127.0.0.1:{}", persistent_2.local_addr().unwrap().port());

        spawn(move || {
            for i in 0..500 {
                let mut message = OutgoingMessage::new();
                message.write_usize(i);
                persistent_1
                    .send_to_address(address_2.clone(), message)
                    .ok();
                persistent_1.pump().unwrap();

                sleep(Duration::from_secs_f32(1.0 / 60.0));
            }

            // Continue pumping to ensure messages are resent
            loop {
                persistent_1.pump().unwrap();
                sleep(Duration::from_secs_f32(1.0 / 60.0));
            }
        });

        let mut incoming_messages = Vec::new();
        // Add 10 extra frames to ensure messages finish being recieved
        for _ in 0..510 {
            let pumped_messages =
                persistent_2
                    .pump()
                    .unwrap()
                    .into_iter()
                    .filter_map(|(frame_event, _)| match frame_event {
                        PersistentEvent::FrameCompleted(_, message) => Some(message),
                        _ => None,
                    });
            for mut incoming_message in pumped_messages {
                let id = incoming_message.read_usize();
                incoming_messages.push(id);
            }

            sleep(Duration::from_secs_f32(1.0 / 60.0));
        }

        assert_eq!(incoming_messages.len(), 500);
    }
}
