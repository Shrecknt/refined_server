#![feature(int_roundings)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc, thread, time::Duration};

use azalea::prelude::*;
use futures_channel::mpsc::UnboundedSender;
use parking_lot::{deadlock, Mutex};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    Pool, Postgres, Row,
};
use tokio_tungstenite::tungstenite::Message;

mod find_blocks;

mod minecraft_handle;

mod postgres;
use postgres::{create_chest, items_in_chest, set_item_in_chest};

mod handle_websockets;

mod bot_handle_queue;
use bot_handle_queue::bot_handle_queue;

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

    let account: Account =
        Account::microsoft(&std::env::var("MINECRAFT_EMAIL").expect("MINECRAFT_EMAIL must be set"))
            .await
            .unwrap();

    ClientBuilder::new()
        .set_handler(minecraft_handle::minecraft_handle)
        .start(account, "localhost:25590")
        .await
        .unwrap();

    Ok(())
}
