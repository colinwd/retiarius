use std::net::UdpSocket;

use clap::Parser;
use filters::{drop_chance::DropChance, Filter};

mod filters;

pub const BUFFER_SIZE: usize = 1500;
type Datagram = [u8; BUFFER_SIZE];

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let incoming = UdpSocket::bind(format!("0.0.0.0:{}", args.incoming_port))?;
    let outgoing = UdpSocket::bind(format!("127.0.0.1:{}", args.outgoing_port))?;

    let drop_filter = args.drop_percent.map(|drop_percent| {
        DropChance { drop_percent }
    });

    loop {
        let mut buf = [0; BUFFER_SIZE];
        let (_, src) = incoming.recv_from(&mut buf)?;

        if drop_filter.is_some() {
            let send = drop_filter.unwrap().apply(buf);
            if send.is_none() {
                // filter applies, skip this packet!
                continue;
            }
        }

        outgoing.send_to(&buf, &src)?;
    }
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
