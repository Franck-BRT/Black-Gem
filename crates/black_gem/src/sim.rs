//! Pont entre la simulation déterministe (`gem_sim`) et le moteur.
//!
//! La simulation avance à pas fixe (30 ticks/s) dans `FixedUpdate`, quelle que soit
//! la fréquence d'affichage : c'est ce qui permettra le multijoueur en lockstep.

use bevy::prelude::*;
use gem_sim::{Simulation, TICKS_PER_SECOND};

pub const MAP_SIZE: u32 = 192;
pub const FIRST_SEED: u64 = 2026;

#[derive(Resource, Deref, DerefMut)]
pub struct Sim(pub Simulation);

/// Vitesses de jeu disponibles, comme dans les RTS classiques.
pub const SPEEDS: [f32; 3] = [1.0, 5.0, 20.0];

#[derive(Resource, Default)]
pub struct GameSpeed {
    pub index: usize,
    pub paused: bool,
}

/// Demande de génération d'une nouvelle planète.
#[derive(Message)]
pub struct NewPlanet;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Sim(Simulation::new(FIRST_SEED, MAP_SIZE)))
            .insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
            .init_resource::<GameSpeed>()
            .add_message::<NewPlanet>()
            .add_systems(FixedUpdate, step_simulation)
            .add_systems(Update, (speed_controls, request_new_planet));
    }
}

fn step_simulation(mut sim: ResMut<Sim>) {
    sim.step();
}

fn speed_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut speed: ResMut<GameSpeed>,
    mut time: ResMut<Time<Virtual>>,
) {
    if keys.just_pressed(KeyCode::Space) {
        speed.paused = !speed.paused;
    }
    for (i, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .into_iter()
        .enumerate()
    {
        if keys.just_pressed(key) {
            speed.index = i;
            speed.paused = false;
        }
    }
    if speed.is_changed() {
        if speed.paused {
            time.pause();
        } else {
            time.unpause();
            time.set_relative_speed(SPEEDS[speed.index]);
        }
    }
}

fn request_new_planet(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim: ResMut<Sim>,
    mut new_planet: MessageWriter<NewPlanet>,
) {
    if keys.just_pressed(KeyCode::KeyN) {
        let seed = sim.map.seed + 1;
        sim.0 = Simulation::new(seed, MAP_SIZE);
        new_planet.write(NewPlanet);
    }
}
