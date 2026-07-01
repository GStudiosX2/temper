pub mod errors;
pub mod heightmap;
pub mod light;
pub mod network;
mod palette;
pub mod section;
pub mod vanilla_chunk_format;

use crate::errors::WorldError;
use crate::heightmap::Heightmaps;
use crate::section::{AIR, ChunkSection};
use dashmap::DashMap;
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use temper_core::block_state_id::BlockStateId;
use temper_core::pos::{ChunkBlockPos, ChunkHeight};
use temper_entities::entity_types::EntityTypeEnum;
use temper_macros::block;
use type_hash::TypeHash;
use uuid::Uuid;
use vanilla_chunk_format::VanillaChunk;

#[derive(Clone, Serialize, Deserialize, TypeHash)]
pub struct Chunk {
    pub sections: Box<[ChunkSection]>,
    height: ChunkHeight,
    #[type_hash(foreign_type)]
    pub entities: DashMap<Uuid, (EntityTypeEnum, Vec<u8>)>,

    heightmaps: Option<Heightmaps>,
    dirty: Arc<AtomicBool>,
}

impl Chunk {
    /// Returns a chunk that is completely filled with air.
    ///
    /// This uses the overworld [`ChunkHeight`] (-64..320) as the chunk's height.
    ///
    /// # Returns
    ///
    /// * An empty chunk filled with air using the overworld [`ChunkHeight`].
    pub fn new_empty() -> Chunk {
        Self::new_empty_with_height(ChunkHeight::new(-64, 384))
    }

    /// Returns a chunk that is completely filled with air.
    ///
    /// # Arguments
    ///
    /// * `height` - The [`ChunkHeight`] that this chunk should be set to
    ///
    /// # Returns
    ///
    /// * An empty chunk filled with air using the given [`ChunkHeight`].
    pub fn new_empty_with_height(height: ChunkHeight) -> Chunk {
        Self {
            sections: vec![ChunkSection::new_uniform(AIR); (height.height / 16) as usize]
                .into_boxed_slice(),
            height,
            entities: DashMap::new(),
            heightmaps: None,
            dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a chunk using the given sections and height.
    ///
    /// # Arguments
    ///
    /// * `sections` - The sections to fill the chunk with. These should be in order from the bottom of the world at index 0 and the top at the end of the slice.
    /// * `height` - The [`ChunkHeight`] to use.
    ///
    /// # Asserts
    ///
    /// * debug_assert_eq: `sections` contains enough [`ChunkSection`]s to fill the chunk based on the given [`ChunkHeight`].
    ///
    /// # Returns
    ///
    /// * A chunk using the given sections and [`ChunkHeight`]
    pub fn new_with_sections(sections: &[ChunkSection], height: ChunkHeight) -> Chunk {
        debug_assert_eq!(height.height as usize / 16, sections.len());

        Self {
            sections: sections.to_vec().into_boxed_slice(),
            height,
            heightmaps: None,
            entities: DashMap::new(),
            dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Fills an entire [`ChunkSection`] with the given block.
    ///
    /// # Arguments
    ///
    /// * `y` - The y of the section to fill.
    /// * `state` - The [`BlockStateId`] to fill the section with.
    ///
    /// # Asserts
    ///
    /// * `assert` - Checks if the given y value is in range of the height of the chunk.
    pub fn fill_section(&mut self, y: i8, state: BlockStateId) {
        assert!(i16::from(y) >= self.height.min_y / 16);
        assert!(i16::from(y) < (self.height.min_y + self.height.height as i16) / 16);

        let section = i16::from(y) + -self.height.min_y / 16;

        self.sections[section as usize] = ChunkSection::new_uniform(state);
    }

    /// Fills the entire chunk with the given block.
    ///
    /// # Arguments
    ///
    /// * `state` - The [`BlockStateId`] of the block to fill the chunk with.
    pub fn fill(&mut self, state: BlockStateId) {
        for section in &mut self.sections {
            *section = ChunkSection::new_uniform(state);
        }
    }

    /// Gets a block in the chunk.
    ///
    /// # Arguments
    ///
    /// * `pos` - The position of the block to get.
    ///
    /// # Returns
    ///
    /// * The [`BlockStateId`] of the block at the requested position. If the position is above the maximum y of the chunk, air is always returned.
    ///   If the position is below the minimum y of the chunk, void air is always returned.
    pub fn get_block(&self, pos: ChunkBlockPos) -> BlockStateId {
        let section = (pos.y() + -self.height.min_y) / 16;
        if section < 0 {
            return block!("void_air");
        }

        if section as usize >= self.sections.len() {
            return block!("air");
        }

        self.sections[section as usize].get_block(pos.section_block_pos())
    }

    /// Sets a block in the chunk.
    ///
    /// # Arguments
    ///
    /// * `pos` - The position of the block to set within the chunk.
    /// * `id` - The [`BlockStateId`] of the block to set.
    ///
    /// # Asserts
    ///
    /// * `assert` - Checks to ensure that the given position is in-bounds.
    pub fn set_block(&mut self, pos: ChunkBlockPos, id: BlockStateId) {
        let section = (pos.y() + -self.height.min_y) / 16;
        assert!(section >= 0);
        assert!(section as usize <= self.sections.len());

        self.sections[section as usize].set_block(pos.section_block_pos(), id);
    }

    /// Get heightmap for entire chunk
    pub fn get_full_heightmap(&self) -> [i16; 256usize] {
        let mut heightmap = [0i16; 256];

        for x in 0..=15 {
            for z in 0..=15 {
                heightmap[x as usize + (z as usize * 16)] = self.get_heightmap(x, z);
            }
        }

        heightmap
    }

    /// Gets the highest Y in the chunk
    pub fn get_heightmap(&self, x: u8, z: u8) -> i16 {
        debug_assert!(x <= 16 && z <= 16);

        for (section_index, section) in self.sections.iter().enumerate().rev() {
            let height = section.world_surface.get_height(x, z);
            if height != 0 {
                return self.height.min_y + (section_index as i16 * 16) + i16::from(height) - 1;
            }
        }

        0
    }

    /// Marks the chunk as dirty.
    ///
    /// This indicates that the chunk has been modified and may need to be saved or updated.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Checks if the chunk is dirty.
    ///
    /// A chunk is considered dirty if it has been marked as dirty or if any of its sections are dirty.
    /// A dirty chunk may need to be saved or updated.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(std::sync::atomic::Ordering::Relaxed)
            || self
                .sections
                .iter()
                .any(|s| s.dirty.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Clears the dirty state of the chunk and all of its sections.
    ///
    /// This should be called after saving or updating a chunk to indicate that it is no longer dirty.
    pub fn clear_dirty(&self) {
        self.dirty
            .store(false, std::sync::atomic::Ordering::Relaxed);
        for section in &self.sections {
            section
                .dirty
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl TryFrom<&VanillaChunk> for Chunk {
    type Error = WorldError;

    fn try_from(value: &VanillaChunk) -> Result<Self, Self::Error> {
        let mut sections = vec![ChunkSection::new_uniform(AIR); 24];

        if value.status != "minecraft:full" {
            return Err(WorldError::CorruptedChunkData(0, 0));
        }

        for section in value
            .sections
            .as_ref()
            .ok_or(WorldError::CorruptedChunkData(
                value.x_pos as _,
                value.z_pos as _,
            ))?
            .iter()
        {
            sections[(section.y + 4).clamp(0, 23) as usize] = ChunkSection::try_from(section)?;
        }

        Ok(Chunk {
            sections: sections.into_boxed_slice(),
            height: ChunkHeight::new(-64, 384),
            heightmaps: value
                .heightmaps
                .as_ref()
                .and_then(|v| Heightmaps::try_from(v).ok()),
            entities: DashMap::new(),
            dirty: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::BlockStateId;
    use crate::Chunk;
    use temper_core::pos::ChunkBlockPos;
    use temper_macros::block;

    #[test]
    fn test_read_write() {
        let mut chunk = Chunk::new_empty();

        chunk.set_block(ChunkBlockPos::new(0, 0, 0), block!("stone"));
        chunk.set_block(ChunkBlockPos::new(0, 16, 1), block!("dirt"));

        assert_eq!(
            chunk.get_block(ChunkBlockPos::new(0, 0, 0)),
            block!("stone")
        );
        assert_eq!(
            chunk.get_block(ChunkBlockPos::new(0, 16, 1)),
            block!("dirt")
        );
    }
}
