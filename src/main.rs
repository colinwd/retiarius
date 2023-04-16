use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
};

use bytes::Bytes;
use clap::Parser;
use tokio::{
    io::Interest,
    net::UdpSocket,
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};

pub const BUFFER_SIZE: usize = 1500;

#[derive(Debug)]
struct Datagram {
    payload: Bytes,
    origin: SocketAddr,
    destination: Option<SocketAddr>,
}

impl Datagram {
    fn set_destination(mut self, destination: SocketAddr) -> Self {
        self.destination = Some(destination);
        return self;
    }
}

struct ChanneledSocket {
    producer: Sender<Datagram>,
}

impl ChanneledSocket {
    /// Create a new ChanneledSocket, injecting its UdpSocket and a sender that determines where it routes traffic to.
    async fn new(socket: UdpSocket, sender: Sender<Datagram>) -> ChanneledSocket {
        println!("starting channeled socket for {:?}", socket.local_addr());
        
        let (producer, mut receiver) = channel::<Datagram>(100);

        // receiver recv -> socket send
        // socket recv -> sender send
        tokio::spawn(async move {
            loop {
                let ready = socket
                    .ready(Interest::READABLE | Interest::WRITABLE)
                    .await
                    .expect("failed to determine read/write interest of socket");

                if ready.is_readable() {
                    let mut data = [0; BUFFER_SIZE];

                    if let Ok((len, origin)) = &socket.try_recv_from(&mut data[..]) {
                        println!("received {:?} from {}", &data[..*len], origin);

                        let bytes = Bytes::copy_from_slice(&data[..*len]);
                        let datagram = Datagram {
                            payload: bytes,
                            origin: *origin,
                            destination: None,
                        };

                        println!("Sending {:?} on channel", datagram);
                        sender
                            .send(datagram)
                            .await
                            .expect("failed to send message on channel");
                    }
                }

                if ready.is_writable() {
                    if let Ok(message) = receiver.try_recv() {
                        println!("sending {:?} to {}", &message.payload, message.origin);
                        socket
                            .send_to(&message.payload, message.origin)
                            .await
                            .expect("failed to send on socket");
                    }
                }
            }
        });

        ChanneledSocket { producer }
    }

    /// Get the input sender of this ChanneledSocket
    fn get_input_sender(&self) -> Sender<Datagram> {
        return self.producer.clone();
    }
}

struct Router;

struct Route {
    channeled_socket: ChanneledSocket,
    receiver: Receiver<Datagram>,
}

impl Router {
    fn new(
        server_addr: SocketAddr,
        client_sender: Sender<Datagram>,
        mut client_receiver: Receiver<Datagram>,
    ) -> JoinHandle<()> {
        let mut routes = HashMap::new();

        tokio::spawn(async move {
            loop {
                if let Ok(message) = client_receiver.try_recv() {
                    println!("router received message {:?}", message);

                    if let Entry::Vacant(_) = routes.entry(message.origin) {
                        let proxy_socket = UdpSocket::bind(("127.0.0.1", 0))
                            .await
                            .expect("unable to bind proxy socket");

                        proxy_socket
                            .connect(server_addr)
                            .await
                            .expect("failed to connect proxy socket to server address");

                        println!("proxy socket created on port {}", proxy_socket.local_addr().unwrap().port());

                        let (router_sender, proxy_receiver) = channel::<Datagram>(100);
                        let channeled_socket =
                            ChanneledSocket::new(proxy_socket, router_sender).await;
                        let route = Route {
                            channeled_socket,
                            receiver: proxy_receiver,
                        };

                        routes.insert(message.origin, route);
                    }

                    if let Some(route) = routes.get(&message.origin) {
                        println!("sending message to proxy socket");
                        route
                            .channeled_socket
                            .get_input_sender()
                            .send(message)
                            .await
                            .expect("failed to send message from router to proxy socket");
                    }
                }

                for (addr, route) in routes.iter_mut() {
                    let receiver = &mut route.receiver;
                    if let Ok(message) = receiver.try_recv() {
                        println!("Received message from proxy socket: {:?}", message);
                        let message = message.set_destination(*addr);
                        client_sender
                            .send(message)
                            .await
                            .expect("failed to send to client sender");
                    }
                }
            }
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let client_socket = UdpSocket::bind(("0.0.0.0", args.listen_port))
        .await
        .expect("unable to bind client socket");

    let (router_sender, client_receiver) = channel::<Datagram>(100);

    let client_socket = ChanneledSocket::new(client_socket, router_sender.clone()).await;

    let router = Router::new(
        args.server_addr,
        client_socket.get_input_sender(),
        client_receiver,
    );

    let _join = tokio::join!(router);

    Ok(())
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
