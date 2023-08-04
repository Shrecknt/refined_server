use std::collections::LinkedList;
use std::{collections::HashMap, sync::Arc};

use azalea::container::ContainerHandle;
use azalea::inventory::operations::QuickMoveClick;
use azalea::prelude::*;
use azalea::BlockPos;
use azalea_inventory::ItemSlot;
use parking_lot::Mutex;
use serde::Deserialize;
use sqlx::PgPool;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::net::TcpListener;

use crate::find_blocks::find_blocks;
use crate::handle_websockets::handle_connection0;
use crate::postgres::{create_chest, set_item_in_chest};
use crate::{bot_handle_queue, PeerMap};

#[derive(Default, Clone, Component)]
pub struct State {
    pub checked_chests: Arc<Mutex<Vec<BlockPos>>>,
}

#[derive(Deserialize, Debug, Clone, Component)]
pub struct Config {
    pub bot_owner: String,
    pub region: Region,
}
#[derive(Deserialize, Debug, Clone)]
pub struct Region {
    pub walking_level: i32,
    pub x1: i32,
    pub z1: i32,
    pub x2: i32,
    pub z2: i32,
    pub min_y: i32,
    pub max_y: i32,
}

pub async fn minecraft_handle(
    mut bot: azalea::Client,
    event: Event,
    state: State,
) -> anyhow::Result<()> {
    match event {
        Event::Init => {
            let config: Config =
                toml::from_str(&tokio::fs::read_to_string("config.toml").await.unwrap()).unwrap();
            println!("Loaded config: {config:?}");
            bot.ecs.lock().entity_mut(bot.entity).insert(config.clone());

            let pool: Pool<Postgres> = PgPoolOptions::new()
                .max_connections(5)
                .connect(&format!(
                    "postgres://postgres:{}@localhost/chest_storage",
                    std::env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD must be set")
                ))
                .await?;

            bot.ecs
                .lock()
                .entity_mut(bot.entity)
                .insert(PostgresComponent { pool: pool.clone() });

            let queue = Arc::new(Mutex::new(LinkedList::new()));
            let queue = WebsocketQueue {
                queue: Arc::clone(&queue),
            };

            bot.ecs.lock().entity_mut(bot.entity).insert(queue.clone());

            let addr = "127.0.0.1:8080".to_string();
            let state = PeerMap::new(Mutex::new(HashMap::new()));
            let listener = TcpListener::bind(addr)
                .await
                .expect("Unable to bind, is the port already in use?");
            let queue2 = queue.clone();
            tokio::spawn(async move {
                let queue = queue2;
                loop {
                    let (stream, addr) = listener.accept().await.unwrap();
                    tokio::spawn(handle_connection0(
                        state.clone(),
                        stream,
                        addr,
                        queue.clone(),
                    ));
                }
            });
            tokio::spawn(async move {
                bot_handle_queue::bot_handle_queue(queue.clone(), bot, config, pool)
                    .await
                    .unwrap();
            });
            return Ok(());
        }
        _ => {}
    };

    // println!("AAA");
    let pool: PgPool = bot.component::<PostgresComponent>().pool;
    // println!("BBB");
    let queue: Arc<Mutex<LinkedList<String>>> = bot.component::<WebsocketQueue>().queue;
    // println!("CCC");
    // let config: Config = bot.component::<Config>();
    // println!("DDD");

    match event {
        Event::Chat(m) => {
            println!("{}", m.message());

            if m.username() == Some(bot.profile.name.clone()) {
                return Ok(());
            };

            if m.username() == Some(/*config.bot_owner*/ "Shrecknt".to_string()) {
                if m.content().starts_with("$") {
                    let content = m.content();
                    let mut command = content.chars();
                    command.next();
                    queue.lock().push_back(command.as_str().to_string());
                }
            }

            if m.content() != "go" {
                return Ok(());
            }

            {
                state.checked_chests.lock().clear();
            }

            let chest_blocks = find_blocks(
                bot.world().read(),
                bot.position(),
                &azalea::Block::Chest.into(),
            );
            println!("Found chests at {:?}", chest_blocks);

            for chest_block in chest_blocks {
                if bot.position().distance_to(&chest_block.to_vec3_floored()) > 10.0 {
                    continue;
                }
                // bot.goto(BlockPosGoal::from(chest_block));
                let mut chest: Option<ContainerHandle> = bot.open_container(chest_block).await;
                let mut retries = 3;
                while retries > 0 && chest.is_none() {
                    retries -= 1;
                    chest = bot.open_container(chest_block).await;
                }
                let chest = match chest {
                    Some(chest) => chest,
                    None => continue,
                };

                create_chest(
                    &pool,
                    chest_block.x as f64,
                    chest_block.y as f64,
                    chest_block.z as f64,
                )
                .await?;

                println!("Getting contents");
                for (index, slot) in chest
                    .contents()
                    .expect("we just opened the chest")
                    .iter()
                    .enumerate()
                {
                    set_item_in_chest(
                        &pool,
                        chest_block.x as f64,
                        chest_block.y as f64,
                        chest_block.z as f64,
                        index.try_into().unwrap_or(-1),
                        &slot.kind().to_string(),
                        slot.count() as i16,
                        None,
                    )
                    .await
                    .unwrap();

                    println!("Checking slot {index}: {slot:?}");
                    if let ItemSlot::Present(item) = slot {
                        bot.chat(&format!("found item: [{} x{}]", item.kind, item.count));

                        if item.kind == azalea::Item::Diamond
                            || item.kind == azalea::Item::DiamondBlock
                        {
                            println!("clicking slot ^");
                            chest.click(QuickMoveClick::Left { slot: index as u16 });
                        }
                    }
                }
            }
            println!("Done");
            bot.chat("Done");
        }
        _ => {}
    }

    Ok(())
}

#[derive(Clone, Component)]
struct PostgresComponent {
    pool: Pool<Postgres>,
}

#[derive(Clone, Component)]
pub struct WebsocketQueue {
    pub queue: Arc<Mutex<LinkedList<String>>>,
}
