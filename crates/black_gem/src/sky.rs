//! Cycle jour/nuit : soleil, lumière ambiante, couleur du ciel et du brouillard.
//! L'heure vient de la simulation, qui reste la seule source de vérité.

use std::f32::consts::{PI, TAU};

use bevy::{
    light::{CascadeShadowConfigBuilder, light_consts::lux},
    pbr::DistanceFog,
    prelude::*,
};

use crate::sim::Sim;

#[derive(Component)]
struct Sun;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight::default())
            .add_systems(Startup, spawn_sun)
            .add_systems(Update, update_sky);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Sun,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            maximum_distance: 260.0,
            first_cascade_far_bound: 30.0,
            ..default()
        }
        .build(),
    ));
}

/// Palette du ciel selon la hauteur du soleil (-1 = minuit, 0 = horizon, 1 = zénith).
fn sky_color(sun_height: f32) -> Color {
    let night = Vec3::new(0.012, 0.018, 0.05);
    let dusk = Vec3::new(0.75, 0.36, 0.3);
    let day = Vec3::new(0.42, 0.6, 0.76);
    let c = if sun_height < 0.0 {
        night.lerp(dusk, (1.0 + sun_height / 0.25).clamp(0.0, 1.0))
    } else {
        dusk.lerp(day, (sun_height / 0.35).clamp(0.0, 1.0))
    };
    Color::srgb(c.x, c.y, c.z)
}

fn update_sky(
    sim: Res<Sim>,
    mut sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fog: Single<&mut DistanceFog>,
) {
    // 0 = minuit, 0.25 = lever, 0.5 = midi, 0.75 = coucher.
    let day_fraction = sim.time_of_day() as f32 / 1000.0;
    let angle = (day_fraction - 0.25) * TAU;
    let sun_height = angle.sin();

    let (light, transform) = &mut *sun;
    if sun_height > -0.05 {
        // Le soleil traverse le ciel d'est en ouest, légèrement incliné vers le sud.
        let direction = Vec3::new(angle.cos(), angle.sin().max(0.02), 0.35).normalize();
        transform.translation = direction * 100.0;
        transform.look_at(Vec3::ZERO, Vec3::Y);
        let strength = (sun_height / 0.3).clamp(0.0, 1.0);
        light.illuminance = lux::AMBIENT_DAYLIGHT * strength;
        let warm = 1.0 - (sun_height / 0.4).clamp(0.0, 1.0);
        light.color = Color::srgb(1.0, 0.95 - 0.35 * warm, 0.88 - 0.55 * warm);
    } else {
        // La nuit, une lune froide prend le relais (sens opposé au soleil).
        let moon_angle = angle + PI;
        let direction = Vec3::new(moon_angle.cos(), moon_angle.sin().max(0.1), -0.3).normalize();
        transform.translation = direction * 100.0;
        transform.look_at(Vec3::ZERO, Vec3::Y);
        light.illuminance = 900.0;
        light.color = Color::srgb(0.55, 0.65, 1.0);
    }

    let daylight = (sun_height / 0.3).clamp(0.0, 1.0);
    ambient.brightness = 120.0 + 480.0 * daylight;
    ambient.color = Color::srgb(0.55 + 0.45 * daylight, 0.6 + 0.4 * daylight, 1.0);

    let sky = sky_color(sun_height);
    clear.0 = sky;
    fog.color = sky;
}
