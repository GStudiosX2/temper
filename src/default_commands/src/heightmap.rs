use bevy_ecs::prelude::*;
use bevy_math::Vec3A;
use temper_commands::Sender;
use temper_components::player::{position::Position, rotation::Rotation};
use temper_core::pos::BlockPos;
use temper_macros::command;
use temper_messages::{particle::SendParticle, teleport_player::TeleportPlayer};
use temper_net_runtime::connection::StreamWriter;
use temper_particles::ParticleType;
use temper_state::GlobalStateResource;

#[command("heightmap")]
#[allow(unused_mut)]
#[allow(clippy::cast_lossless)]
fn heightmap_command(
    #[sender] sender: Sender,
    args: (
        Query<(&Rotation, &Position, &StreamWriter)>,
        Res<GlobalStateResource>,
        MessageWriter<TeleportPlayer>,
        MessageWriter<SendParticle>,
    ),
) {
    let (mut query, global_state, mut _tp_player_msg, mut send_particle) = args;

    let player_entity = match sender {
        Sender::Server => {
            sender.send_message("Error: Need a entity.".into(), false);
            return;
        }
        Sender::Player(entity) => entity,
    };

    let (_rot, pos, _conn) = query.get(player_entity).unwrap();
    let mut chunk = global_state
        .0
        .world
        .get_chunk_mut(pos.chunk(), temper_core::dimension::Dimension::Overworld)
        .unwrap();

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

    // let height = chunk.get_heightmap(pos.x.rem_euclid(16f64) as u8, pos.z.rem_euclid(16f64) as u8);
    // let resolved_pos = Position::new(pos.x, f64::from(height) + 1f64, pos.z);
    // tp_player_msg.write(TeleportPlayer {
    //     entity: player_entity,
    //     x: resolved_pos.x,
    //     y: resolved_pos.y,
    //     z: resolved_pos.z,
    //     vel_x: 0.0,
    //     vel_y: 0.0,
    //     vel_z: 0.0,
    //     yaw: rot.yaw,
    //     pitch: rot.pitch,
    // });
    // sender.send_message(format!("Teleported to ({}).", resolved_pos).into(), false);
}
