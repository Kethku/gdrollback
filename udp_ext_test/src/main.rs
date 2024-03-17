use udp_ext::{frame::FrameSocket, messages::OutgoingMessage};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_host = args.get(1) == Some(&"host".to_owned());
    let port = if is_host { 11337 } else { 0 };
    let mut socket = FrameSocket::bind(port).expect("Could not bind port");

    if !is_host {
        let host_address = "home.kaylees.dev:11337";
        let mut message = OutgoingMessage::new();
        message.write_string("Did it work?");
        socket
            .send_to(message, host_address)
            .expect("Could not send message");
    }

    loop {
        if let Ok(results) = socket.pump() {
            for (event, address) in results {
                dbg!(event, address);
            }
        }
    }
}
