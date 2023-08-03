#![feature(int_roundings)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use azalea::prelude::*;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::StreamExt, TryStreamExt};
use parking_lot::Mutex;
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    Pool, Postgres, Row,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

mod find_blocks;

mod minecraft_handle;
use minecraft_handle::minecraft_handle;

mod postgres;
use postgres::{create_chest, items_in_chest, set_item_in_chest};

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect(&format!(
            "postgres://postgres:{}@localhost/chest_storage",
            std::env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD must be set")
        ))
        .await?;

    println!("AAA");
    sqlx::query("DELETE FROM chest_items")
        .fetch_optional(&pool)
        .await?;
    println!("BBB");
    sqlx::query("DELETE FROM chests")
        .fetch_optional(&pool)
        .await?;
    println!("CCC");

    let res: Vec<PgRow> =
        sqlx::query("SELECT * FROM get_items_from_chest ($1::float, $2::float, $3::float);")
            .bind(1 as f64)
            .bind(2 as f64)
            .bind(3 as f64)
            .fetch_all(&pool)
            .await?;
    println!(
        "res = {:?}",
        res.iter()
            .map(|item| item.try_get("item_id").unwrap_or("UNKNOWN"))
            .collect::<Vec<_>>()
    );

    create_chest(&pool, 1f64, 2f64, 3f64).await?;

    set_item_in_chest(&pool, 1f64, 2f64, 3f64, 0, "minecraft:stone", 64, None).await?;
    set_item_in_chest(
        &pool,
        1f64,
        2f64,
        3f64,
        1,
        "minecraft:cobblestone",
        64,
        None,
    )
    .await?;

    println!(
        "items in chest: {:?}",
        items_in_chest(&pool, 1f64, 2f64, 3f64)
            .await?
            .iter()
            .map(|item| item.try_get::<_, &str>("item_id").unwrap_or("UNKNOWN"))
            .collect::<Vec<_>>()
    );

    println!(
        "chests rows: {:?}\nchest_items rows: {:?}",
        sqlx::query("SELECT * FROM chests")
            .fetch_all(&pool)
            .await?
            .iter()
            .map(|item| item.columns())
            .collect::<Vec<_>>(),
        sqlx::query("SELECT * FROM chest_items")
            .fetch_all(&pool)
            .await?
            .iter()
            .map(|item| item.columns())
            .collect::<Vec<_>>()
    );

    tokio::spawn(async move {
        let account: Account = Account::microsoft(
            &std::env::var("MINECRAFT_EMAIL").expect("MINECRAFT_EMAIL must be set"),
        )
        .await
        .unwrap();

        ClientBuilder::new()
            .set_handler(minecraft_handle)
            .start(account, "localhost:25590")
            .await
            .unwrap();
    });

    let addr = "127.0.0.1:8080".to_string();
    let state = PeerMap::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind(addr)
        .await
        .expect("Unable to bind, is the port already in use?");
    loop {
        let (stream, addr) = listener.accept().await?;
        tokio::spawn(handle_connection0(state.clone(), stream, addr));
    }
}

async fn handle_connection0(peer_map: PeerMap, stream: TcpStream, addr: SocketAddr) {
    handle_connection(peer_map, stream, addr).await.unwrap();
}

async fn handle_connection(
    peer_map: PeerMap,
    stream: TcpStream,
    addr: SocketAddr,
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
