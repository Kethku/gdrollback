use std::collections::*;
use std::io::{Error, ErrorKind};
use std::net::UdpSocket;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use super::messages::*;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PartialOrd, Ord)]
pub struct PacketId(usize);

#[derive(Debug, PartialEq)]
pub enum ReliableEvent {
    PacketAcknowledged(PacketId),
    PacketResent(PacketId),
    PacketRecieved(IncomingMessage),
}

struct UnackedMessage {
    pub packet_id: PacketId,
    pub message: OutgoingMessage,
    pub destination: SocketAddr,
    pub last_sent: Option<Instant>,
}

impl UnackedMessage {
    pub const RESEND_MILLIS: u64 = 32;

    pub fn new(
        packet_id: PacketId,
        message: OutgoingMessage,
        destination: SocketAddr,
    ) -> UnackedMessage {
        UnackedMessage {
            packet_id,
            message,
            destination,
            last_sent: None,
        }
    }

    pub fn send_if_needed(
        &mut self,
        socket: &UdpSocket,
    ) -> Result<Option<(ReliableEvent, SocketAddr)>, Error> {
        if self.last_sent.is_none() {
            socket.send_to(&self.message.data, self.destination)?;
            self.last_sent = Some(Instant::now());
            return Ok(None);
        }

        let time_since_last_sent = self.last_sent.unwrap().elapsed();

        if time_since_last_sent > Duration::from_millis(UnackedMessage::RESEND_MILLIS) {
            socket.send_to(&self.message.data, self.destination)?;
            self.last_sent = Some(Instant::now());

            Ok(Some((
                ReliableEvent::PacketResent(self.packet_id),
                self.destination,
            )))
        } else {
            Ok(None)
        }
    }
}

pub struct ReliableSocket {
    socket: UdpSocket,
    packet_id_counter: AtomicUsize,
    unacked_messages: HashMap<PacketId, (UnackedMessage, Instant)>,
    seen_acks: HashMap<SocketAddr, BTreeSet<PacketId>>,
}

impl ReliableSocket {
    pub const MAX_RELIABLE_PACKET_SIZE: usize = 500;

    pub fn bind(port: u16) -> Result<ReliableSocket> {
        dbg!(port);
        let socket = UdpSocket::bind(("0.0.0.0", port))?;
        socket.set_nonblocking(true)?;

        Ok(ReliableSocket {
            socket,
            packet_id_counter: AtomicUsize::new(0),
            unacked_messages: HashMap::new(),
            seen_acks: HashMap::new(),
        })
    }

    fn resend_unacked_messages(&mut self) -> Result<Vec<(ReliableEvent, SocketAddr)>> {
        let mut results = Vec::new();

        for (_, (unacked_message, _)) in self.unacked_messages.iter_mut() {
            if let Some(event) = unacked_message.send_if_needed(&self.socket)? {
                results.push(event);
            }
        }

        Ok(results)
    }

    fn send_ack(&mut self, packet_id: PacketId, destination: SocketAddr) -> Result<(), Error> {
        let mut ack_message = OutgoingMessage::new();
        ack_message.write_bool(false);
        ack_message.write_usize(packet_id.0);

        self.socket.send_to(&ack_message.data, destination)?;
        Ok(())
    }

    pub fn send_to(
        &mut self,
        message: OutgoingMessage,
        destination: impl ToSocketAddrs,
    ) -> Result<PacketId, Error> {
        let destination = destination.to_socket_addrs()?.next().unwrap();
        if message.data.len() > ReliableSocket::MAX_RELIABLE_PACKET_SIZE {
            return Err(Error::new(ErrorKind::InvalidData, "Packet too large."));
        }

        let packet_id = PacketId(self.packet_id_counter.fetch_add(1, Ordering::Relaxed));
        let mut wrapped_message = OutgoingMessage::new();
        wrapped_message.write_bool(true);
        wrapped_message.write_usize(packet_id.0);

        wrapped_message.write_data(message.data);

        let mut unacked_message = UnackedMessage::new(packet_id, wrapped_message, destination);
        unacked_message.send_if_needed(&self.socket)?;
        let _ = self
            .unacked_messages
            .insert(packet_id, (unacked_message, Instant::now()));
        Ok(packet_id)
    }

    pub fn pump(&mut self) -> Result<Vec<(ReliableEvent, SocketAddr)>> {
        let mut results = self.resend_unacked_messages()?;

        let mut buf = [0u8; ReliableSocket::MAX_RELIABLE_PACKET_SIZE];
        while let Ok((byte_count, remote_address)) = self.socket.recv_from(&mut buf) {
            dbg!(&remote_address);

            let mut incoming_message = IncomingMessage::new(buf[..byte_count].to_vec());
            let is_data = incoming_message
                .read_bool()
                .ok_or(anyhow!("Reliable message is not data."))?;
            let packet_id = PacketId(
                incoming_message
                    .read_usize()
                    .ok_or(anyhow!("Reliable message does not have ack."))?,
            );
            if is_data {
                self.send_ack(packet_id, remote_address)?;
                if self
                    .seen_acks
                    .get(&remote_address)
                    .map_or(true, |seen_acks| !seen_acks.contains(&packet_id))
                {
                    results.push((
                        ReliableEvent::PacketRecieved(incoming_message),
                        remote_address,
                    ));
                    let seen_acks = self
                        .seen_acks
                        .entry(remote_address)
                        .or_insert_with(|| BTreeSet::new());
                    seen_acks.insert(packet_id);
                    while seen_acks.len() > 1000 {
                        seen_acks.pop_first();
                    }
                }
            } else if let Some(_) = self.unacked_messages.remove(&packet_id) {
                results.push((ReliableEvent::PacketAcknowledged(packet_id), remote_address));
            }
        }

        Ok(results)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.socket.local_addr()
    }
}

#[cfg(test)]
mod test {
    use std::net::UdpSocket;
    use std::thread::sleep;
    use std::time::Duration;

    use anyhow::Result;

    use super::*;

    #[test]
    fn reliable_socket_resends() {
        let mut reliable = ReliableSocket::bind(0).unwrap();
        let reliable_address = format!("127.0.0.1:{}", reliable.local_addr().unwrap().port());
        let test = UdpSocket::bind("127.0.0.1:0").unwrap();
        test.set_nonblocking(true).unwrap();
        let test_address = test.local_addr().unwrap();
        let test_message = "This is a test.";

        let mut message = OutgoingMessage::new();
        message.write_string(test_message);
        let ack_id = reliable.send_to(message, test_address).unwrap();

        sleep(Duration::from_millis(210));
        assert!(matches!(
            reliable.pump().unwrap().pop().unwrap(),
            (ReliableEvent::PacketResent(id), address)
                if id == ack_id && address == test_address
        ));

        sleep(Duration::from_millis(210));
        assert!(matches!(
            reliable.pump().unwrap().pop().unwrap(),
            (ReliableEvent::PacketResent(id), address)
                if id == ack_id && address == test_address
        ));

        let mut buf = [0u8; ReliableSocket::MAX_RELIABLE_PACKET_SIZE];
        let (byte_count, _) = test.recv_from(&mut buf).unwrap();
        let mut incoming_message = IncomingMessage::new(buf[..byte_count].to_vec());
        assert_eq!(incoming_message.read_bool().unwrap(), true);
        assert_eq!(incoming_message.read_usize().unwrap(), ack_id.0);
        assert_eq!(&incoming_message.read_string().unwrap(), test_message);

        let mut ack = OutgoingMessage::new();
        ack.write_bool(false);
        ack.write_usize(ack_id.0);
        test.send_to(&ack.data, reliable_address).unwrap();

        assert!(matches!(reliable.pump().unwrap().pop().unwrap(),
        (ReliableEvent::PacketAcknowledged(id), address)
            if id == ack_id && address == test_address));
        sleep(Duration::from_millis(210));

        assert!(reliable.pump().unwrap().is_empty());
    }

    #[test]
    fn reliable_socket_acknowledges() -> Result<()> {
        let mut reliable = ReliableSocket::bind(0)?;
        let reliable_address = format!("127.0.0.1:{}", reliable.local_addr().unwrap().port());
        let test = UdpSocket::bind("127.0.0.1:0")?;
        test.set_nonblocking(true)?;
        let test_address = test.local_addr()?;
        let test_message = "This is a test.";

        let mut message = OutgoingMessage::new();
        message.write_bool(true); // Message Type (content)
        message.write_usize(42); // Ack Id
        message.write_string(test_message); // Message Data
        test.send_to(&message.data, reliable_address)?;

        sleep(Duration::from_millis(100));

        let mut events = reliable.pump()?;
        if let (ReliableEvent::PacketRecieved(mut incoming_message), address) =
            events.pop().expect("Recieved Event")
        {
            assert_eq!(incoming_message.read_string().unwrap(), test_message);
            assert_eq!(address, test_address);
        } else {
            panic!("reliable socket did not recieve ack.")
        }

        let mut buf = [0u8; ReliableSocket::MAX_RELIABLE_PACKET_SIZE];
        let (byte_count, _) = test.recv_from(&mut buf)?;
        let mut incoming_message = IncomingMessage::new(buf[..byte_count].to_vec());
        assert_eq!(incoming_message.read_bool().unwrap(), false); // Message Type (ack)
        assert_eq!(incoming_message.read_usize().unwrap(), 42); // Ack Id

        Ok(())
    }
}
