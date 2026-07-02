use bevy_ecs::prelude::*;
use bevy_math::Vec3A;
use temper_command_infra::{CommandHandler, CommandResult, CommandSource};
use temper_components::player::{position::Position, rotation::Rotation};
use temper_core::pos::BlockPos;
use temper_macros::Command;
use temper_messages::particle::SendParticle;
use temper_net_runtime::connection::StreamWriter;
use temper_particles::ParticleType;
use temper_permissions::Permissions;
use temper_state::GlobalStateResource;

#[derive(Command)]
#[command(name ="heightmap", permission = Permissions::ALL)]
struct HeightmapCommand {}

impl CommandHandler for HeightmapCommand {
    type SystemParam<'w, 's> = (
        Query<'w, 's, (&'static Rotation, &'static Position, &'static StreamWriter)>,
        Res<'w, GlobalStateResource>,
        MessageWriter<'w, SendParticle>,
    );

    fn handle(
        self,
        source: CommandSource,
        params: &mut Self::SystemParam<'_, '_>,
    ) -> CommandResult {
        let &mut (ref mut query, ref global_state, ref mut send_particle) = params;
    
        let player_entity = match source {
            CommandSource::Server => {
                return Err("This command can only be ran as a player.".into());
            }
            CommandSource::Player(entity) => entity,
        };
    
        let (_rot, pos, _conn) = query.get(player_entity).unwrap();
        let chunk = global_state
            .0
            .world
            .get_chunk_mut(pos.chunk(), temper_core::dimension::Dimension::Overworld)
            .expect("invalid chunk");
    
        for x in 0..=15 {
            for z in 0..=15 {
                let height = chunk.get_heightmap(x, z) + 1;
                let pos = pos
                    .chunk()
                    .chunk_block(BlockPos::of(x as i32, height as i32, z as i32).chunk_block_pos());
                
                for offset in [
                    Vec3A::ZERO,
                    Vec3A::new(0f32, 0f32, 1f32),
                    Vec3A::new(1f32, 0f32, 0f32),
                    Vec3A::new(1f32, 0f32, 1f32),
                ] {
                    send_particle.write(SendParticle {
                        particle_type: ParticleType::EndRod,
                        position: Vec3A::new(pos.pos.x as f32, pos.pos.y as f32, pos.pos.z as f32)
                            + offset,
                        offset: Vec3A::ZERO,
                        speed: 0.0,
                        count: 1,
                    });
                }
            }
        }
        
        Ok(())
    }
}