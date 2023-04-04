use std::{
    net::SocketAddr,
    sync::Arc,
};

use bytes::Bytes;
use clap::Parser;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver, Sender},
};

mod filters;

pub const BUFFER_SIZE: usize = 1500;
type Datagram = [u8; BUFFER_SIZE];

struct BufferedSocket {
    socket: Arc<UdpSocket>,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
}

impl BufferedSocket {
    async fn new(
        address: &str,
        port: u16,
        sender: Sender<Message>,
        receiver: Receiver<Message>,
    ) -> BufferedSocket {
        BufferedSocket {
            socket: Arc::new(
                UdpSocket::bind(format!("{}:{}", address, port))
                    .await
                    .unwrap(),
            ),
            sender,
            receiver,
        }
    }
}

#[derive(Debug)]
struct Message {
    destination: SocketAddr,
    payload: Bytes,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let (front_sender, back_receiver) = mpsc::channel::<Message>(1024);
    let (back_sender, front_receiver) = mpsc::channel::<Message>(1024);

    let front =
        BufferedSocket::new("0.0.0.0", args.incoming_port, front_sender, front_receiver).await;
    let back =
        BufferedSocket::new("127.0.0.1", args.outgoing_port, back_sender, back_receiver).await;

    let front_socket = front.socket.clone();
    let front_tx = front.sender.clone();
    let front_send_task = tokio::spawn(async move {
        let mut buf = [0; BUFFER_SIZE];
        loop {
            let (len, addr) = front_socket.recv_from(&mut buf).await.unwrap();
            println!("Got {} bytes from {}", len, addr);

            let message = Message {
                destination: addr,
                payload: Bytes::copy_from_slice(&buf[..len]),
            };

            front_tx.send(message).await.unwrap();
        }
    });

    let back_socket = back.socket.clone();
    let back_tx = back.sender.clone();
    let back_send_task = tokio::spawn(async move {
        let mut buf = [0; BUFFER_SIZE];
        loop {
            let (len, addr) = back_socket.recv_from(&mut buf).await.unwrap();
            println!("Got {} bytes from {}", len, addr);

            let message = Message {
                destination: addr,
                payload: Bytes::copy_from_slice(&buf[..len]),
            };

            back_tx.send(message).await.unwrap();
        }
    });

    let front_socket = front.socket.clone();
    let mut front_rx = front.receiver;
    let front_recv_task = tokio::spawn(async move {
        loop {
            let received = front_rx.recv().await;

            if let Some(message) = received {
                println!(
                    "Received {} bytes from front channel destined for {}",
                    message.payload.len(),
                    message.destination
                );
                front_socket
                    .send_to(message.payload.as_ref(), message.destination)
                    .await
                    .unwrap();
            }
        }
    });

    let back_socket = back.socket.clone();
    let mut back_rx = back.receiver;
    let back_recv_task = tokio::spawn(async move {
        loop {
            let received = back_rx.recv().await;

            if let Some(message) = received {
                println!(
                    "Received {} bytes from back channel destined for {}",
                    message.payload.len(),
                    message.destination
                );
                back_socket
                    .send_to(message.payload.as_ref(), message.destination)
                    .await
                    .unwrap();
            }
        }
    });

    tokio::join!(
        front_send_task,
        back_send_task,
        front_recv_task,
        back_recv_task
    );

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    incoming_port: u16,

    #[arg(short, long)]
    outgoing_port: u16,

    /// A value between 0 and 1 representing the percentage chance to drop any given packet.
    /// 0 will never drop a packet, 1 will always drop a packet.
    #[arg(long)]
    drop_percent: Option<f64>,
}
