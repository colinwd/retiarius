use std::{collections::{HashMap, hash_map::Entry}, net::SocketAddr};

use clap::Parser;
use tokio::{io::ErrorKind::WouldBlock, io::Interest, net::UdpSocket};

mod filters;

pub const BUFFER_SIZE: usize = 1500;
type Datagram = [u8; BUFFER_SIZE];

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let socket = UdpSocket::bind(("0.0.0.0", args.listen_port))
        .await
        .expect("failed to bind proxy socket");

    let mut address_map: HashMap<SocketAddr, UdpSocket> = HashMap::new();

    loop {
        let ready = socket
            .ready(Interest::READABLE)
            .await
            .expect("failed to determine read interest of proxy socket");

        let mut data = [0; BUFFER_SIZE];

        if ready.is_readable() {
            match socket.try_recv_from(&mut data[..]) {
                Ok((len, origin)) => {
                    println!("received {:?} from {}", &data[..len], origin);

                    if let Entry::Vacant(_) = address_map.entry(origin) {
                        let client_socket = get_available_socket()
                            .await
                            .expect("unable to bind client socket");

                        client_socket
                            .connect(args.server_addr)
                            .await
                            .expect("failed to connect to server_addr");

                        address_map.insert(origin, client_socket);
                    }

                    let destination_socket = address_map.get(&origin);
                    if let Some(s) = destination_socket {
                        s.send(&data[..len])
                            .await
                            .expect("connection to server_addr failed");
                    } else {
                        println!("no destination socket found with address {}", origin);
                    }
                }
                Err(ref e) if e.kind() == WouldBlock => {}
                Err(e) => {
                    panic!("read from proxy socket failed: {e:?}");
                }
            }
        }
    }
}

async fn get_available_socket() -> Option<UdpSocket> {
    for port in 1025..65535 {
        match UdpSocket::bind(("127.0.0.1", port)).await {
            Ok(socket) => return Some(socket),
            Err(_) => continue,
        }
    }

    None
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The port to listen for client traffic on
    #[arg(long)]
    listen_port: u16,

    /// The address to forward client traffic to
    #[arg(long)]
    server_addr: SocketAddr,
    // /// A value between 0 and 1 representing the percentage chance to drop any given packet.
    // /// 0 will never drop a packet, 1 will always drop a packet.
    // #[arg(long)]
    // drop_percent: Option<f64>,
}
