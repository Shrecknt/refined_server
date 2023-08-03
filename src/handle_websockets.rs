use std::net::SocketAddr;

use futures_channel::mpsc::unbounded;
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use tokio::net::TcpStream;

use crate::{minecraft_handle::WebsocketQueue, PeerMap};

pub async fn handle_connection0(
    peer_map: PeerMap,
    stream: TcpStream,
    addr: SocketAddr,
    queue: WebsocketQueue,
) {
    handle_connection(peer_map, stream, addr, queue)
        .await
        .unwrap();
}

pub async fn handle_connection(
    peer_map: PeerMap,
    stream: TcpStream,
    addr: SocketAddr,
    queue: WebsocketQueue,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    let (tx, rx) = unbounded();
    peer_map.lock().insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        println!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );
        queue
            .queue
            .lock()
            .push_back(msg.to_text().unwrap_or("BAD_MESSAGE").to_string());

        let peers = peer_map.lock();

        // We want to broadcast the message to everyone except ourselves.
        let broadcast_recipients = peers
            .iter()
            .filter(|(peer_addr, _)| peer_addr != &&addr)
            .map(|(_, ws_sink)| ws_sink);

        for recp in broadcast_recipients {
            recp.unbounded_send(msg.clone()).unwrap();
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    peer_map.lock().remove(&addr);

    Ok(())
}
