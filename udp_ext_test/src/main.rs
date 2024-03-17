use std::net::ToSocketAddrs;
use udp_ext::{messages::OutgoingMessage, persistent::PersistentSocket};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_host = args.get(1) == Some(&"host".to_owned());
    let port = if is_host { 11337 } else { 0 };
    let mut socket = PersistentSocket::<usize>::bind(port).expect("Could not bind port");

    if !is_host {
        let host_address = "home.kaylees.dev:11337"
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        socket.connect(0, host_address);
        let mut message = OutgoingMessage::new();
        message.write_string("Did it work?");
        socket.send_to(0, message).expect("Could not send message");
    }

    loop {
        if let Ok(results) = socket.pump() {
            for (event, address) in results {
                dbg!(event, address);
            }
        }
    }
}
