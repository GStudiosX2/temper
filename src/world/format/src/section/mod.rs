use crate::errors::WorldError;
use crate::light::{LightStorage, SectionLightData};
use crate::section::biome::{BiomeData, BiomeType};
use crate::section::direct::DirectSection;
use crate::section::paletted::{PalettedSection, PalettedSectionResult};
use crate::section::uniform::UniformSection;
use crate::vanilla_chunk_format::Section;
use deepsize::DeepSizeOf;
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use temper_core::block_state_id::BlockStateId;
use temper_core::pos::{ChunkBlockPos, SectionBlockPos};
use temper_macros::block;
use type_hash::TypeHash;

mod biome;
mod direct;
pub mod network;
mod paletted;
mod uniform;

pub const CHUNK_SECTION_LENGTH: usize = 16 * 16 * 16;

pub(crate) const AIR: BlockStateId = block!("air");

#[derive(Clone, DeepSizeOf, Serialize, Deserialize, TypeHash)]
pub(crate) enum ChunkSectionType {
    Uniform(UniformSection),
    Paletted(PalettedSection),
    Direct(DirectSection),
}

impl ChunkSectionType {
    #[inline]
    pub fn get_block(&self, pos: SectionBlockPos) -> BlockStateId {
        let pos = pos.pack() as usize;

        match self {
            Self::Uniform(data) => data.get_block(),
            Self::Paletted(data) => data.get_block(pos),
            Self::Direct(data) => data.get_block(pos),
        }
    }

    #[inline]
    pub fn set_block(&mut self, pos: SectionBlockPos, id: BlockStateId) {
        let pos = pos.pack() as usize;

        match self {
            Self::Uniform(data) => {
                // Check if the id doesn't match the block type that fills the section,
                // If not, then create a PalettedSection to hold more than one block type
                if id != data.get_block() {
                    let mut new_data = PalettedSection::from(data);
                    new_data.set_block(pos, id);
                    *self = Self::Paletted(new_data);
                }
            }
            Self::Paletted(data) => match data.set_block(pos, id) {
                // Shrink the PalettedSection into a UniformSection if one block fills the entire section
                PalettedSectionResult::Shrink(block) => {
                    *self = ChunkSectionType::Uniform(UniformSection::new_with(block))
                }
                // Expand the PalettedSection into a DirectSection if more than u8::MAX block types are in the section
                PalettedSectionResult::Expand => {
                    let mut new_data = DirectSection::from(data);
                    new_data.set_block(pos, id);
                    *self = Self::Direct(new_data);
                }
                PalettedSectionResult::Keep => {}
            },
            Self::Direct(data) => data.set_block(pos, id),
        }
    }

    #[inline]
    pub fn fill(&mut self, id: BlockStateId) {
        match self {
            Self::Uniform(data) => data.fill(id),
            _ => *self = Self::Uniform(UniformSection::new_with(id)),
        }
    }

    pub fn block_count(&self) -> u16 {
        match self {
            Self::Uniform(data) => {
                if data.get_block() == AIR {
                    0
                } else {
                    4096
                }
            }
            Self::Paletted(data) => data.block_count(),
            Self::Direct(data) => data.block_count(),
        }
    }
}

const UNIFORM_NON_EMPTY_HEIGHTMAP: [u8; 256] = [16u8; 256];

#[derive(Default, Clone, DeepSizeOf, Serialize, Deserialize, TypeHash)]
pub enum SectionHeightmap {
    #[default]
    Air,
    Uniform(bool),
    Full(Box<[u8]>),
}

impl SectionHeightmap {
    pub fn get_height(&self, x: u8, z: u8) -> u8 {
        match self {
            Self::Air => 0,
            Self::Uniform(is_not_air) => {
                if *is_not_air {
                    16
                } else {
                    0
                }
            }
            Self::Full(heights) => heights[((z << 4) | x) as usize],
        }
    }

    pub fn set_height(&mut self, x: u8, z: u8, height: u8) {
        let index = ((z << 4) | x) as usize;

        match self {
            SectionHeightmap::Air | SectionHeightmap::Uniform(false) => {
                *self = SectionHeightmap::Full(Box::new([0u8; 256]))
            }
            SectionHeightmap::Uniform(true) => {
                *self = SectionHeightmap::Full(Box::new(UNIFORM_NON_EMPTY_HEIGHTMAP))
            }
            _ => {}
        }

        match self {
            SectionHeightmap::Full(data) => data[index] = height,
            _ => unreachable!(),
        }
    }

    pub fn heights(&self) -> Option<&[u8]> {
        match self {
            Self::Air => None,
            Self::Uniform(is_not_air) => {
                if *is_not_air {
                    Some(&UNIFORM_NON_EMPTY_HEIGHTMAP)
                } else {
                    None
                }
            }
            Self::Full(heightmap) => Some(heightmap),
        }
    }
}

#[derive(Clone, DeepSizeOf, Serialize, Deserialize, TypeHash)]
pub struct ChunkSection {
    pub(crate) inner: ChunkSectionType,
    pub(crate) light: SectionLightData,
    pub(crate) biome: BiomeData,
    pub(crate) dirty: Arc<AtomicBool>,
    // #[serde(skip_serializing, skip_deserializing)]
    pub(crate) world_surface: SectionHeightmap,
}

impl ChunkSection {
    pub fn new_uniform(id: BlockStateId) -> Self {
        Self {
            inner: ChunkSectionType::Uniform(UniformSection::new_with(id)),
            world_surface: if id == AIR {
                SectionHeightmap::Air
            } else {
                SectionHeightmap::Uniform(true)
            },
            light: SectionLightData::default(),
            biome: BiomeData::Uniform(BiomeType(5)),
            dirty: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn with_space_for(unique_blocks: u16) -> Self {
        if unique_blocks <= 1 {
            Self {
                inner: ChunkSectionType::Uniform(UniformSection::air()),
                world_surface: SectionHeightmap::Air,
                light: SectionLightData::default(),
                biome: BiomeData::Uniform(BiomeType(5)),
                dirty: Arc::new(AtomicBool::new(true)),
            }
        } else if unique_blocks < 256 {
            Self {
                inner: ChunkSectionType::Paletted(PalettedSection::new_with_block_count(
                    unique_blocks as _,
                )),
                world_surface: SectionHeightmap::Air,
                light: SectionLightData::default(),
                biome: BiomeData::Uniform(BiomeType(5)),
                dirty: Arc::new(AtomicBool::new(true)),
            }
        } else {
            Self {
                inner: ChunkSectionType::Direct(DirectSection::default()),
                world_surface: SectionHeightmap::Air,
                light: SectionLightData::default(),
                biome: BiomeData::Uniform(BiomeType(5)),
                dirty: Arc::new(AtomicBool::new(true)),
            }
        }
    }

    #[inline]
    pub fn get_block(&self, pos: SectionBlockPos) -> BlockStateId {
        self.inner.get_block(pos)
    }

    #[inline]
    pub fn set_block(&mut self, pos: SectionBlockPos, id: BlockStateId) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        self.inner.set_block(pos, id);

        if let ChunkSectionType::Uniform(_) = &self.inner {
            self.world_surface = SectionHeightmap::Uniform(self.inner.block_count() != 0);
        } else {
            // y + 1 because 0 = no heightmap for this xz in the section
            self.update_single_heightmap(pos.x, pos.z, pos.y + 1, id);
        }
    }

    #[inline]
    pub fn fill(&mut self, id: BlockStateId) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        self.inner.fill(id);
        self.world_surface = SectionHeightmap::Uniform(self.inner.block_count() != 0);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.fill(AIR)
    }

    #[inline]
    pub fn block_count(&self) -> u16 {
        self.inner.block_count()
    }

    fn update_single_heightmap(&mut self, x: u8, z: u8, height: u8, id: BlockStateId) {
        let current_height = self.world_surface.get_height(x, z);

        if id == AIR {
            if height >= current_height {
                let mut section_pos =
                    ChunkBlockPos::new(x, i16::from(current_height.max(height)), z)
                        .section_block_pos();
                loop {
                    if self.inner.get_block(section_pos) != AIR {
                        self.world_surface.set_height(x, z, section_pos.y + 1);
                        break;
                    }

                    if section_pos.y == 0 {
                        self.world_surface.set_height(x, z, 0);
                        break;
                    }

                    section_pos.y -= 1;
                }
            }
        } else {
            if height >= current_height {
                self.world_surface.set_height(x, z, height);
            }
        }
    }
}

impl TryFrom<&Section> for ChunkSection {
    type Error = WorldError;

    fn try_from(value: &Section) -> Result<Self, Self::Error> {
        let sky_light = value
            .sky_light
            .clone()
            .map(LightStorage::from)
            .unwrap_or_default();
        let block_light = value
            .block_light
            .clone()
            .map(LightStorage::from)
            .unwrap_or_default();

        let light_data = SectionLightData::with_data(sky_light, block_light);

        if let Some(block_data) = value.block_states.as_ref() {
            let (block_count, block_states) = if let Some(blocks) = block_data.data.as_ref() {
                if let Some(palette) = block_data.palette.as_ref() {
                    let bits_per_block =
                        ((palette.len().saturating_sub(1) as u32).ilog2() + 1).max(4);

                    let mut values = Vec::with_capacity(4096);

                    for i in 0..4096 {
                        values.push(PalettedSection::unpack_value_unaligned(
                            bytemuck::cast_slice(blocks.as_slice()),
                            i,
                            bits_per_block as _,
                        ))
                    }

                    debug_assert_eq!(values.len(), 4096);

                    (
                        if bits_per_block >= 9 {
                            None
                        } else {
                            Some(palette.len())
                        },
                        values
                            .into_iter()
                            .map(|v| {
                                if bits_per_block >= 9 {
                                    BlockStateId::new(v.into())
                                } else {
                                    BlockStateId::from_block_data(&palette[v as usize])
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    return Err(WorldError::CorruptedChunkData(0, 0));
                }
            } else {
                return Ok(Self {
                    light: light_data,
                    biome: BiomeData::Uniform(BiomeType(5)),
                    dirty: Arc::new(AtomicBool::new(false)),

                    inner: ChunkSectionType::Uniform(UniformSection::air()),
                    world_surface: SectionHeightmap::Air,
                });
            };

            let mut section_data = if let Some(block_count) = block_count {
                ChunkSectionType::Paletted(PalettedSection::new_with_block_count(block_count as _))
            } else {
                ChunkSectionType::Direct(DirectSection::default())
            };

            for (idx, block) in block_states.into_iter().enumerate() {
                section_data.set_block(
                    SectionBlockPos::unpack(idx as _).expect("should be in-bounds"),
                    block,
                )
            }

            Ok(Self {
                light: light_data,
                biome: BiomeData::Uniform(BiomeType(5)),
                dirty: Arc::new(AtomicBool::new(false)),
                inner: section_data,

                world_surface: SectionHeightmap::Air, // TODO: make section heightmap from vanilla section
            })
        } else {
            Ok(Self {
                light: light_data,
                biome: BiomeData::Uniform(BiomeType(5)),
                dirty: Arc::new(AtomicBool::new(false)),
                inner: ChunkSectionType::Uniform(UniformSection::air()),
                world_surface: SectionHeightmap::Air,
            })
        }
    }
}
