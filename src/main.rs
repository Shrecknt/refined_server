#![feature(int_roundings)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc, thread, time::Duration};

use azalea::prelude::*;
use config::CONFIG;
use futures_channel::mpsc::UnboundedSender;
use parking_lot::{deadlock, Mutex};
use tokio_tungstenite::tungstenite::Message;

mod bot_handle_queue;
mod config;
mod find_blocks;
mod handle_websockets;
mod minecraft_handle;
mod postgres;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let deadlocks = deadlock::check_deadlock();
        if deadlocks.is_empty() {
            continue;
        }

        println!("{} deadlocks detected", deadlocks.len());
        for (i, threads) in deadlocks.iter().enumerate() {
            println!("Deadlock #{}", i);
            for t in threads {
                println!("Thread Id {:#?}", t.thread_id());
                println!("{:#?}", t.backtrace());
            }
        }
    });

    dotenv::dotenv().ok();

    let account: Account =
        Account::microsoft(&std::env::var("MINECRAFT_EMAIL").expect("MINECRAFT_EMAIL must be set"))
            .await
            .unwrap();

    ClientBuilder::new()
        .set_handler(minecraft_handle::minecraft_handle)
        .start(account, CONFIG.connections.remote_host.as_str())
        .await
        .unwrap();

    Ok(())
}
