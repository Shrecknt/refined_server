use std::time::Duration;

use azalea::entity::Position;
use azalea::protocol::packets::game::ServerboundGamePacket;
use azalea::{
    prelude::ContainerClientExt,
    protocol::packets::game::serverbound_move_player_pos_packet::ServerboundMovePlayerPosPacket,
};
use azalea_inventory::operations::QuickMoveClick;
use azalea_inventory::ItemSlot;
use sqlx::{PgPool, Row};

use crate::{
    find_blocks::find_blocks,
    minecraft_handle::{Config, Region, WebsocketQueue},
    postgres::{create_chest, find_item, items_in_chest, set_item_in_chest},
};

pub async fn bot_handle_queue(
    queue: WebsocketQueue,
    mut bot: azalea::Client,
    config: Config,
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    match bot_handle_queue0(queue, &mut bot, config, pool).await {
        Err(err) => {
            bot.chat("The queue thread died, check logs");
            println!("Error: {}", err);
        }
        Ok(()) => unreachable!(),
    };
    Ok(())
}
pub async fn bot_handle_queue0(
    queue: WebsocketQueue,
    bot: &mut azalea::Client,
    config: Config,
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let region: Region = config.region;

    loop {
        let command = queue.queue.lock().pop_front();
        let command = match command {
            Some(command) => command,
            None => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let command = command.clone();
        let command_arr = command.split(' ').collect::<Vec<_>>();

        println!("Recieved command: {}", command);

        match command_arr[0] {
            "sayhi" => {
                bot.chat("hi");
            }
            "index" => {
                bot.chat("Indexing...");

                let storage_blocks = find_blocks(
                    bot.world().read(),
                    bot.position(),
                    &azalea::Block::Barrel.into(),
                );
                let storage_blocks = storage_blocks
                    .iter()
                    .filter(|block| {
                        block.y >= region.min_y
                            && block.y <= region.max_y
                            && block.x >= region.x1
                            && block.x <= region.x2
                            && block.z >= region.z1
                            && block.z <= region.z2
                    })
                    .map(|block| block.clone())
                    .collect::<Vec<_>>();

                // for block in storage_blocks {
                //     bot.chat(&format!("Storage block: {:?}", block.to_vec3_floored()));
                // }
                bot.chat(&format!("Indexing {} storage blocks", storage_blocks.len()));
                'blocks: for block in storage_blocks {
                    bot.write_packet(ServerboundGamePacket::MovePlayerPos(
                        ServerboundMovePlayerPosPacket {
                            x: block.x as f64 + 0.5,
                            y: region.walking_level as f64,
                            z: block.z as f64 + 0.5,
                            on_ground: true,
                        },
                    ));

                    {
                        let mut ecs = bot.ecs.lock();
                        let mut entity_mut = ecs.entity_mut(bot.entity);
                        let mut position = entity_mut.get_mut::<Position>().unwrap();
                        position.x = block.x as f64 + 0.5;
                        position.z = block.z as f64 + 0.5;
                    }

                    let mut barrel = bot.open_container(block).await;
                    let mut retries = 5;
                    while barrel.is_none() && retries > 0 {
                        bot.chat("retrying");
                        retries -= 1;
                        barrel = bot.open_container(block).await;
                        match barrel {
                            Some(_) => {
                                bot.chat("retry successful");
                            }
                            None => {
                                bot.chat(&format!(
                                    "retry failed, {} attempt(s) remaining",
                                    retries
                                ));
                            }
                        }
                    }
                    let barrel = match barrel {
                        Some(barrel) => barrel,
                        None => {
                            println!("failed to open storage block at [{:?}] for an unknown reason (this is probably my fault)", block);
                            continue 'blocks;
                        }
                    };

                    create_chest(&pool, block.x as f64, block.y as f64, block.z as f64).await?;

                    println!("Getting contents");
                    for (index, slot) in match barrel.contents() {
                        Some(contents) => contents,
                        None => {
                            bot.chat(&format!(
                                "skipping storage block at [{:?}] because of an error (mta pls fix)",
                                block
                            ));
                            continue 'blocks;
                        }
                    }
                    .iter()
                    .enumerate()
                    {
                        set_item_in_chest(
                            &pool,
                            block.x as f64,
                            block.y as f64,
                            block.z as f64,
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
                                barrel.click(QuickMoveClick::Left { slot: index as u16 });
                            }
                        }
                    }

                    drop(barrel);
                    bot.run_schedule_sender.send(())?;
                    // tokio::time::sleep(Duration::from_millis(100)).await;
                }

                bot.chat("Done!");
            }
            "cleardb" => {
                sqlx::query("DELETE FROM chest_items")
                    .fetch_optional(&pool)
                    .await?;
                sqlx::query("DELETE FROM chests")
                    .fetch_optional(&pool)
                    .await?;
                bot.chat("Cleared DB");
            }
            "viewchest" => {
                let x = command_arr[1].parse::<f64>()?;
                let y = command_arr[2].parse::<f64>()?;
                let z = command_arr[3].parse::<f64>()?;
                let res = items_in_chest(&pool, x, y, z).await?;
                if res.len() == 0 {
                    bot.chat("No items found at location");
                }
                for item in res {
                    let item_id: &str = item.get("item_id");
                    if item_id != "minecraft:air" {
                        bot.chat(&format!(
                            "{} x{}",
                            item_id,
                            item.get::<i16, _>("item_count")
                        ));
                    }
                }
            }
            "find" => {
                let item_id = command_arr[1];
                let res = find_item(&pool, item_id).await?;
                for location in res {
                    let item_count = location.get::<i16, _>("item_count");
                    let x = location.get::<f64, _>("x");
                    let y = location.get::<f64, _>("y");
                    let z = location.get::<f64, _>("z");
                    bot.chat(&format!(
                        "Found {}x of {} at ({}, {}, {})",
                        item_count, item_id, x, y, z
                    ));
                }
            }
            _ => {
                bot.chat("unknown command");
            }
        };
    }
}
