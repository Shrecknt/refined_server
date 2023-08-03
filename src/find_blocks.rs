use azalea::blocks::{BlockState, BlockStates};
use azalea::world::iterators::ChunkIterator;
use azalea::world::palette::Palette;
use azalea::world::Instance;
use azalea::BlockPos;
use azalea_core::ChunkPos;

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
