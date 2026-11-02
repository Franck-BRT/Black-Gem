//! Cycle jour/nuit : soleil, lumière ambiante, couleur du ciel et du brouillard.
//! L'heure vient de la simulation, qui reste la seule source de vérité.

use std::f32::consts::{PI, TAU};

use bevy::{
    light::{CascadeShadowConfigBuilder, NotShadowCaster, light_consts::lux},
    pbr::DistanceFog,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use crate::{camera::RtsCamera, sim::Sim};

#[derive(Component)]
struct Sun;

/// Le dôme du ciel, centré sur la caméra.
#[derive(Component)]
struct SkyDome;

/// Rayon du dôme : à l'intérieur du plan lointain de la caméra (1000).
const DOME_RADIUS: f32 = 900.0;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
struct SkyMaterial {
    #[uniform(0)]
    sun: Vec4,
    #[uniform(0)]
    zenith: Vec4,
    #[uniform(0)]
    horizon: Vec4,
    #[uniform(0)]
    sun_color: Vec4,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://black_gem/shaders/sky.wesl".into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/sky.wesl");
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .insert_resource(GlobalAmbientLight::default())
            .add_systems(Startup, (spawn_sun, spawn_dome))
            .add_systems(Update, (update_sky, follow_camera));
    }
}

fn spawn_dome(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    // Sphère vue de l'intérieur : on inverse le sens des triangles.
    let mesh = Sphere::new(DOME_RADIUS)
        .mesh()
        .uv(64, 32)
        .with_inverted_winding()
        .expect("sphère triangulée");
    commands.spawn((
        SkyDome,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(SkyMaterial::default())),
        Transform::default(),
        NotShadowCaster,
    ));
}

fn follow_camera(
    camera: Single<&Transform, (With<RtsCamera>, Without<SkyDome>)>,
    mut dome: Single<&mut Transform, With<SkyDome>>,
) {
    dome.translation = camera.translation;
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

/// Couleurs du ciel (zénith, horizon) selon la hauteur du soleil (-1 minuit, 1 zénith).
fn sky_colors(sun_height: f32) -> (Vec3, Vec3) {
    let night = (Vec3::new(0.004, 0.007, 0.022), Vec3::new(0.02, 0.03, 0.07));
    let dusk = (Vec3::new(0.18, 0.14, 0.38), Vec3::new(0.95, 0.45, 0.25));
    let day = (Vec3::new(0.08, 0.24, 0.6), Vec3::new(0.52, 0.7, 0.9));
    let mix = |a: (Vec3, Vec3), b: (Vec3, Vec3), t: f32| (a.0.lerp(b.0, t), a.1.lerp(b.1, t));
    if sun_height < 0.0 {
        mix(night, dusk, (1.0 + sun_height / 0.22).clamp(0.0, 1.0))
    } else {
        mix(dusk, day, (sun_height / 0.35).clamp(0.0, 1.0))
    }
}

#[allow(clippy::too_many_arguments)]
fn update_sky(
    sim: Res<Sim>,
    time: Res<Time<Real>>,
    mut sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fog: Single<&mut DistanceFog>,
    dome: Single<&MeshMaterial3d<SkyMaterial>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
) {
    // 0 = minuit, 0.25 = lever, 0.5 = midi, 0.75 = coucher.
    let day_fraction = sim.time_of_day() as f32 / 1000.0;
    let angle = (day_fraction - 0.25) * TAU;
    let sun_height = angle.sin();
    // Le soleil traverse le ciel d'est en ouest, légèrement incliné vers le sud.
    let sun_direction = Vec3::new(angle.cos(), angle.sin(), 0.35).normalize();

    let (light, transform) = &mut *sun;
    if sun_height > -0.05 {
        let direction = Vec3::new(sun_direction.x, sun_direction.y.max(0.02), sun_direction.z);
        transform.translation = direction.normalize() * 100.0;
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

    let (zenith, horizon) = sky_colors(sun_height);
    // Le brouillard prend la couleur de l'horizon : le sol se fond dans le ciel au loin.
    let horizon_color = Color::srgb(horizon.x, horizon.y, horizon.z);
    clear.0 = horizon_color;
    fog.color = horizon_color;

    if let Some(mut material) = sky_materials.get_mut(&dome.0) {
        let warm = 1.0 - (sun_height / 0.4).clamp(0.0, 1.0);
        let visible = ((sun_height + 0.08) / 0.12).clamp(0.0, 1.0);
        let night = (-sun_height / 0.2).clamp(0.0, 1.0);
        material.sun = sun_direction.extend(visible);
        material.zenith = zenith.extend(night);
        material.horizon = horizon.extend(time.elapsed_secs() % 3600.0);
        material.sun_color = Vec4::new(1.0, 0.9 - 0.3 * warm, 0.75 - 0.45 * warm, 0.0);
    }
}
