use std::collections::*;
use std::io::Error;
use std::net::{SocketAddr, ToSocketAddrs};

use anyhow::{anyhow, Result};

use super::messages::*;
use super::reliable::*;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct FrameId(pub usize);

#[derive(Debug, PartialEq)]
pub struct ComponentPosition {
    pub parent_frame: FrameId,
    pub remaining_components: usize,
}

impl ComponentPosition {
    pub fn new(parent_frame: FrameId, remaining_components: usize) -> ComponentPosition {
        ComponentPosition {
            parent_frame,
            remaining_components,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum FrameEvent {
    PacketAcknowledged(PacketId),
    PacketResent(PacketId),
    FrameComponentRecieved(ComponentPosition),
    FrameCompleted(FrameId, IncomingMessage),
    FrameComponentSent(PacketId),
}

enum AddComponentResult {
    Done(IncomingMessage),
    Unfinished(PartialFrame),
}

struct PartialFrame {
    pub frame_components: HashMap<usize, IncomingMessage>,
    pub remaining_components: usize,
}

impl PartialFrame {
    pub fn new(component_count: usize) -> PartialFrame {
        PartialFrame {
            frame_components: HashMap::with_capacity(component_count),
            remaining_components: component_count,
        }
    }

    fn complete_frame_if_done(mut self) -> AddComponentResult {
        if self.remaining_components > 0 {
            return AddComponentResult::Unfinished(self);
        }

        let mut result = Vec::new();
        for i in 0..self.frame_components.len() {
            result.extend(
                self.frame_components
                    .remove(&i)
                    .expect("Missing frame component")
                    .read_rest(),
            );
        }
        AddComponentResult::Done(IncomingMessage::new(result))
    }

    pub fn add_component(mut self, mut component: IncomingMessage) -> Result<AddComponentResult> {
        let component_position = component
            .read_usize()
            .ok_or(anyhow!("Component doesn't have size"))?;

        self.frame_components.insert(component_position, component);
        self.remaining_components -= 1;

        Ok(self.complete_frame_if_done())
    }
}

pub struct FrameSocket {
    reliable: ReliableSocket,
    frame_id_counter: usize,
    packets_to_send: VecDeque<(OutgoingMessage, SocketAddr)>,
    partial_frames: HashMap<(SocketAddr, FrameId), PartialFrame>,
}

impl FrameSocket {
    pub const MAX_FRAME_PACKET_DATA_SIZE: usize = ReliableSocket::MAX_RELIABLE_PACKET_SIZE - 24;

    pub fn bind(port: u16) -> Result<FrameSocket> {
        let reliable = ReliableSocket::bind(port)?;

        Ok(FrameSocket {
            reliable,
            frame_id_counter: 0,
            packets_to_send: VecDeque::new(),
            partial_frames: HashMap::new(),
        })
    }

    pub fn send_to(
        &mut self,
        message: OutgoingMessage,
        destination: impl ToSocketAddrs,
    ) -> Result<FrameId, Error> {
        let destination = destination.to_socket_addrs()?.next().unwrap();
        let data_length = message.data.len();
        let mut readable_message = message.into_incoming();
        let frame_id = self.frame_id_counter;
        self.frame_id_counter += 1;
        let component_count =
            (data_length as f64 / FrameSocket::MAX_FRAME_PACKET_DATA_SIZE as f64).ceil() as usize;
        for i in 0..component_count {
            let next_component_data =
                readable_message.read_at_most_n_u8s(FrameSocket::MAX_FRAME_PACKET_DATA_SIZE);
            let mut wrapped_message = OutgoingMessage::new();
            wrapped_message.write_usize(frame_id);
            wrapped_message.write_usize(component_count);
            wrapped_message.write_usize(i);
            wrapped_message.write_data(next_component_data);

            self.packets_to_send
                .push_back((wrapped_message, destination));
        }

        Ok(FrameId(frame_id))
    }

    pub fn pump(&mut self) -> Result<Vec<(FrameEvent, SocketAddr)>> {
        let mut results = Vec::new();

        for (message, destination) in self.packets_to_send.drain(..) {
            let packet_id = self.reliable.send_to(message, destination)?;
            results.push((FrameEvent::FrameComponentSent(packet_id), destination));
        }

        for reliable_event in self.reliable.pump()? {
            match reliable_event {
                (ReliableEvent::PacketRecieved(mut message), remote_address) => {
                    let frame_id = FrameId(
                        message
                            .read_usize()
                            .ok_or(anyhow!("Frame message doesn't have frame id"))?,
                    );
                    let component_count = message
                        .read_usize()
                        .ok_or(anyhow!("Frame message doesn't have component count"))?;
                    let frame_key = (remote_address, frame_id);

                    let add_result = self
                        .partial_frames
                        .remove(&frame_key)
                        .unwrap_or_else(|| PartialFrame::new(component_count))
                        .add_component(message)?;

                    match add_result {
                        AddComponentResult::Unfinished(partial) => {
                            results.push((
                                FrameEvent::FrameComponentRecieved(ComponentPosition::new(
                                    frame_id,
                                    partial.remaining_components,
                                )),
                                remote_address,
                            ));
                            self.partial_frames.insert(frame_key, partial);
                        }
                        AddComponentResult::Done(finished_message) => results.push((
                            FrameEvent::FrameCompleted(frame_id, finished_message),
                            remote_address,
                        )),
                    }
                }
                (ReliableEvent::PacketAcknowledged(packet_id), remote_address) => {
                    results.push((FrameEvent::PacketAcknowledged(packet_id), remote_address));
                }
                (ReliableEvent::PacketResent(packet_id), remote_address) => {
                    results.push((FrameEvent::PacketResent(packet_id), remote_address));
                }
            }
        }

        Ok(results)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.reliable.local_addr()?)
    }
}

#[cfg(test)]
mod test {
    use std::thread::sleep;
    use std::time::Duration;

    use anyhow::Result;

    use super::*;

    #[test]
    fn frame_socket_reconstructs_large_packets() -> Result<()> {
        let mut frame_socket = FrameSocket::bind(0)?;
        let mut remote_frame_socket = FrameSocket::bind(0)?;
        let remote_address = format!(
            "127.0.0.1:{}",
            remote_frame_socket.local_addr().unwrap().port()
        );

        let mut message = OutgoingMessage::new();
        for _ in 0..5 {
            message.write_string("This is a long message that ideally should be longer than the minimum message length for a reliable socket.");
        }
        assert!(message.len() > FrameSocket::MAX_FRAME_PACKET_DATA_SIZE);

        frame_socket.send_to(message, remote_address)?;
        let mut send_events = frame_socket.pump()?;
        assert!(matches!(
            send_events.pop(),
            Some((FrameEvent::FrameComponentSent(_), _))
        ));
        assert!(matches!(
            send_events.pop(),
            Some((FrameEvent::FrameComponentSent(_), _))
        ));
        assert!(matches!(send_events.pop(), None));
        sleep(Duration::from_millis(5));
        let mut receive_events = dbg!(remote_frame_socket.pump()?);
        assert!(matches!(
            receive_events.remove(0),
            (
                FrameEvent::FrameComponentRecieved(ComponentPosition {
                    parent_frame: _,
                    remaining_components: 1,
                }),
                _
            )
        ));
        assert!(matches!(
            receive_events.remove(0),
            (FrameEvent::FrameCompleted(_, _), _)
        ));

        Ok(())
    }
}
