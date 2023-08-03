use std::sync::Arc;

use azalea::container::ContainerHandle;
use azalea::inventory::operations::QuickMoveClick;
use azalea::prelude::*;
use azalea::BlockPos;
use azalea_inventory::ItemSlot;
use parking_lot::Mutex;
use sqlx::PgPool;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

use crate::{create_chest, find_blocks::find_blocks, set_item_in_chest};

#[derive(Default, Clone, Component)]
pub struct State {
    pub checked_chests: Arc<Mutex<Vec<BlockPos>>>,
}

pub async fn minecraft_handle(
    mut bot: azalea::Client,
    event: Event,
    state: State,
) -> anyhow::Result<()> {
    match event {
        Event::Init => {
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
                .insert(PostgresComponent { pool });
            return Ok(());
        }
        _ => {}
    };

    let pool: PgPool = bot.component::<PostgresComponent>().pool;

    match event {
        Event::Chat(m) => {
            if m.username() == Some(bot.profile.name.clone()) {
                return Ok(());
            };
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
        }
        _ => {}
    }

    Ok(())
}

#[derive(Clone, Component)]
struct PostgresComponent {
    pool: Pool<Postgres>,
}
