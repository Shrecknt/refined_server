#![feature(int_roundings)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use azalea::{
    app::Plugin,
    blocks::{BlockState, BlockStates},
    prelude::*,
    world::{iterators::ChunkIterator, palette::Palette, Instance},
    BlockPos,
};
use azalea_core::ChunkPos;
use azalea_inventory::operations::QuickMoveClick;
use azalea_inventory::ItemSlot;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::StreamExt, TryStreamExt};
use nbt::Blob;
use parking_lot::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio_postgres::{Client, NoTls, Row};
use tokio_tungstenite::tungstenite::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Tx>>>;

async fn create_chest(
    client: &tokio_postgres::Client,
    x: f64,
    y: f64,
    z: f64,
) -> Result<(), tokio_postgres::Error> {
    client
        .query(
            "INSERT INTO chests (x, y, z) VALUES ($1::float, $2::float, $3::float) ON CONFLICT (x, y, z) DO NOTHING;",
            &[&x, &y, &z],
        )
        .await?;
    Ok(())
}

async fn items_in_chest(
    client: &tokio_postgres::Client,
    x: f64,
    y: f64,
    z: f64,
) -> Result<Vec<Row>, tokio_postgres::Error> {
    client
        .query(
            "SELECT * FROM get_items_from_chest ($1::float, $2::float, $3::float);",
            &[&x, &y, &z],
        )
        .await
}

async fn set_item_in_chest(
    client: &tokio_postgres::Client,
    x: f64,
    y: f64,
    z: f64,
    location_in_chest: i32,
    item_id: &str,
    item_count: i16,
    mut item_nbt: Option<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if item_nbt.is_none() {
        let blob: Blob = Blob::new();
        let mut serialized_nbt: Vec<u8> = vec![];
        blob.to_writer(&mut serialized_nbt)?;
        item_nbt = Some(serialized_nbt);
    }
    client
        .query(
            "CALL insert_item_into_chest ($1::float, $2::float, $3::float, $4::int, $5::text, $6::smallint, $7::bytea);",
            &[&x, &y, &z, &location_in_chest, &item_id, &item_count, &item_nbt],
        )
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let (client, connection) = tokio_postgres::connect(
        &format!(
            "host=localhost user=postgres password={} dbname=chest_storage",
            std::env::var("POSTGRES_PASSWORD").expect("POSTGRES_PASSWORD must be set")
        ),
        NoTls,
    )
    .await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    client.query("DELETE FROM chest_items", &[]).await?;
    client.query("DELETE FROM chests", &[]).await?;

    create_chest(&client, 1f64, 2f64, 3f64).await?;

    set_item_in_chest(&client, 1f64, 2f64, 3f64, 0, "minecraft:stone", 64, None).await?;
    set_item_in_chest(
        &client,
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
        items_in_chest(&client, 1f64, 2f64, 3f64)
            .await?
            .iter()
            .map(|item| item.try_get::<_, &str>("item_id").unwrap_or("UNKNOWN"))
            .collect::<Vec<_>>()
    );

    println!(
        "chests rows: {:?}\nchest_items rows: {:?}",
        client.query("SELECT * FROM chests", &[]).await?,
        client.query("SELECT * FROM chest_items", &[]).await?
    );

    tokio::spawn(async move {
        let account: Account = Account::microsoft(
            &std::env::var("MINECRAFT_EMAIL").expect("MINECRAFT_EMAIL must be set"),
        )
        .await
        .unwrap();

        ClientBuilder::new()
            .add_plugins(PostgresPlugin { client: &client })
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

#[derive(Default, Clone, Component)]
struct State {
    pub checked_chests: Arc<Mutex<Vec<BlockPos>>>,
}

async fn minecraft_handle<'a>(
    mut bot: azalea::Client,
    event: Event,
    state: State,
    client: PostgresResource<'a>,
) -> anyhow::Result<()> {
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
                let Some(chest) = bot.open_container(chest_block).await else {
                    println!("Couldn't open chest");
                    return Ok(());
                };

                create_chest(
                    &client.client,
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
                    println!("Checking slot {index}: {slot:?}");
                    if let ItemSlot::Present(item) = slot {
                        if item.kind == azalea::Item::Diamond {
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

pub fn find_blocks(
    this: parking_lot::lock_api::RwLockReadGuard<'_, parking_lot::RawRwLock, Instance>,
    nearest_to: impl Into<BlockPos>,
    block_states: &BlockStates,
) -> Vec<BlockPos> {
    let mut res = vec![];

    // iterate over every chunk in a 3d spiral pattern
    // and then check the palette for the block state

    let nearest_to: BlockPos = nearest_to.into();
    let start_chunk: ChunkPos = (&nearest_to).into();
    let mut iter = ChunkIterator::new(start_chunk, 32);

    // we do `while` instead of `for` so we can access iter later
    while let Some(chunk_pos) = iter.next() {
        let Some(chunk) = this.chunks.get(&chunk_pos) else {
            // if the chunk isn't loaded then we skip it.
            // we don't just return since it *could* cause issues if there's a random
            // unloaded chunk and then more that are loaded.
            // unlikely but still something to consider, and it's not like this slows it
            // down much anyways.
            continue;
        };

        for (section_index, section) in chunk.read().sections.iter().enumerate() {
            let maybe_has_block = match &section.states.palette {
                Palette::SingleValue(id) => block_states.contains(&BlockState { id: *id }),
                Palette::Linear(ids) => ids
                    .iter()
                    .any(|&id| block_states.contains(&BlockState { id })),
                Palette::Hashmap(ids) => ids
                    .iter()
                    .any(|&id| block_states.contains(&BlockState { id })),
                Palette::Global => true,
            };
            if !maybe_has_block {
                continue;
            }

            for i in 0..4096 {
                let block_state = section.states.get_at_index(i);
                let block_state = BlockState { id: block_state };

                if block_states.contains(&block_state) {
                    let (section_x, section_y, section_z) = section.states.coords_from_index(i);
                    let (x, y, z) = (
                        chunk_pos.x * 16 + (section_x as i32),
                        this.chunks.min_y + (section_index * 16) as i32 + section_y as i32,
                        chunk_pos.z * 16 + (section_z as i32),
                    );
                    let this_block_pos = BlockPos { x, y, z };
                    // this_block_pos is pos of selected block
                    res.push(this_block_pos);
                }
            }
        }
    }
    res
}

#[derive(Clone, Resource)]
struct PostgresResource<'a> {
    pub client: &'a Client,
}
#[derive(Clone)]
struct PostgresPlugin<'a> {
    pub client: &'a Client,
}

impl Plugin for PostgresPlugin<'static> {
    fn build(&self, app: &mut azalea::app::App) {
        app.insert_resource(PostgresResource {
            client: self.client,
        });
    }
}
