use std::{
    collections::{hash_map::Entry, HashMap},
    net::SocketAddr,
    sync::Arc,
};

use bytes::Bytes;
use clap::Parser;
use tokio::{
    io::Interest,
    net::UdpSocket,
    sync::mpsc::{channel, error::TryRecvError, Receiver, Sender},
    task::JoinHandle,
};

pub const BUFFER_SIZE: usize = 1500;

#[derive(Debug, Clone)]
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
    _socket_send: JoinHandle<()>,
    _socket_recv: JoinHandle<()>,
}

impl ChanneledSocket {
    /// Create a new ChanneledSocket, injecting its UdpSocket and a sender that determines where it routes traffic to.
    async fn new(socket: UdpSocket, sender: Sender<Datagram>) -> ChanneledSocket {
        let socket = Arc::new(socket);
        let (producer, mut receiver) = channel::<Datagram>(100);

        // receiver recv -> socket send
        let send_socket = socket.clone();
        let _socket_send = tokio::spawn(async move {
            loop {
                if let Some(message) = receiver.recv().await {
                    println!("sending {:?} to {:?}", &message.payload, message.destination);

                    if let Some(destination) = message.destination {
                        send_socket
                            .send_to(&message.payload, destination)
                            .await
                            .expect("failed to send on socket");
                    } else {
                        unreachable!("message should always have a destination by now");
                    }
                }
            }
        });

        let recv_socket = socket.clone();
        let _socket_recv = tokio::spawn(async move {
            loop {
                let mut data = [0; BUFFER_SIZE];

                if let Ok((len, origin)) = recv_socket.recv_from(&mut data[..]).await {
                    println!("received {:?} from {}", &data[..len], origin);

                    let bytes = Bytes::copy_from_slice(&data[..len]);
                    let datagram = Datagram {
                        payload: bytes,
                        origin: origin,
                        destination: None,
                    };

                    println!("Sending {:?} on channel", datagram);
                    sender
                        .send(datagram)
                        .await
                        .expect("failed to send message on channel");
                }
            }
        });

        ChanneledSocket {
            producer,
            _socket_send,
            _socket_recv,
        }
    }

    /// Get the input sender of this ChanneledSocket
    fn get_input_sender(&self) -> Sender<Datagram> {
        return self.producer.clone();
    }
}

struct Router;

struct Route {
    channeled_socket: ChanneledSocket,
    _recv_task: JoinHandle<()>,
}

impl Router {
    fn new(
        server_addr: SocketAddr,
        client_sender: Sender<Datagram>,
        mut client_receiver: Receiver<Datagram>,
    ) -> JoinHandle<()> {
        let mut routes = HashMap::new();

        let router = tokio::spawn(async move {
            loop {
                if let Some(message) = client_receiver.recv().await {
                    println!("router received message {:?}", message);

                    if let Entry::Vacant(_) = routes.entry(message.origin) {
                        let proxy_socket = UdpSocket::bind(("127.0.0.1", 0))
                            .await
                            .expect("unable to bind proxy socket");

                        proxy_socket
                            .connect(server_addr)
                            .await
                            .expect("failed to connect proxy socket to server address");

                        println!(
                            "proxy socket created on port {:?}",
                            proxy_socket.local_addr()
                        );

                        let (router_sender, mut proxy_receiver) = channel::<Datagram>(100);
                        let channeled_socket =
                            ChanneledSocket::new(proxy_socket, router_sender).await;

                        let client_sender_clone = client_sender.clone();

                        let destination = message.origin.clone();

                        let _recv_task = tokio::spawn(async move {
                            loop {
                                if let Some(received) = proxy_receiver.recv().await {
                                    println!("Received message from proxy socket: {:?}", received);

                                    let received = received.set_destination(destination);
                                    client_sender_clone
                                        .send(received)
                                        .await
                                        .expect("failed to send to client sender");
                                }
                            }
                        });

                        let route = Route {
                            channeled_socket,
                            _recv_task,
                        };

                        routes.insert(message.origin, route);
                    }

                    // forward to server
                    if let Some(route) = routes.get(&message.origin) {
                        println!("sending message to proxy socket");

                        let message = message.set_destination(server_addr);

                        route
                            .channeled_socket
                            .get_input_sender()
                            .send(message)
                            .await
                            .expect("failed to send message from router to proxy socket");
                    }
                }
            }
        });

        router
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    println!("starting up");

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
