use std::net::UdpSocket;

use clap::Parser;

const BUFFER_SIZE: usize = 1500;

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let incoming = UdpSocket::bind(format!("0.0.0.0:{}", args.incoming_port))?;
    let outgoing = UdpSocket::bind(format!("127.0.0.1:{}", args.outgoing_port))?;

    loop {
        let mut buf = [0; BUFFER_SIZE];
        let (_, src) = incoming.recv_from(&mut buf)?;
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
}
