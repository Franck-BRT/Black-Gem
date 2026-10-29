//! Caméra de stratégie : déplacement, rotation, zoom, au clavier, à la souris ou au doigt.
//!
//! Les touches sont lues par position physique : ZQSD sur un clavier AZERTY
//! correspond à WASD sur un clavier QWERTY.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::{
    camera::Hdr,
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    pbr::{DistanceFog, FogFalloff},
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::{sim::Sim, world};

const MIN_DISTANCE: f32 = 8.0;
const MAX_DISTANCE: f32 = 150.0;
const MIN_PITCH: f32 = 0.18; // ~10° au-dessus de l'horizon
const MAX_PITCH: f32 = FRAC_PI_2 - 0.05;

/// Caméra orbitale autour d'un point visé au sol.
#[derive(Component)]
pub struct RtsCamera {
    /// Point visé (au sol), tel qu'affiché.
    pub focus: Vec3,
    /// Angle autour de l'axe vertical, en radians.
    pub yaw: f32,
    /// Angle au-dessus de l'horizon, en radians.
    pub pitch: f32,
    /// Distance entre la caméra et le point visé.
    pub distance: f32,
    /// Valeurs visées : la caméra les rejoint en douceur.
    target_focus: Vec2,
    target_yaw: f32,
    target_pitch: f32,
    target_distance: f32,
}

impl RtsCamera {
    fn new(focus: Vec2) -> Self {
        let (yaw, pitch, distance) = (0.6, 0.62, 70.0);
        Self {
            focus: Vec3::new(focus.x, 0.0, focus.y),
            yaw,
            pitch,
            distance,
            target_focus: focus,
            target_yaw: yaw,
            target_pitch: pitch,
            target_distance: distance,
        }
    }

    /// Recentre la caméra (par exemple sur le QG).
    pub fn look_at(&mut self, focus: Vec2) {
        self.target_focus = focus;
    }
}

pub struct RtsCameraPlugin;

impl Plugin for RtsCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera).add_systems(
            Update,
            (
                keyboard_controls,
                mouse_controls,
                touch_controls,
                apply_camera,
            )
                .chain(),
        );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        RtsCamera::new(Vec2::ZERO),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: Color::srgb(0.45, 0.55, 0.65),
            falloff: FogFalloff::Linear {
                start: 120.0,
                end: 330.0,
            },
            ..default()
        },
        Transform::default(),
    ));
}

fn keyboard_controls(
    keys: Res<ButtonInput<KeyCode>>,
    real_time: Res<Time<Real>>,
    sim: Res<Sim>,
    mut camera: Single<&mut RtsCamera>,
) {
    // Temps réel : la caméra reste fluide même en pause ou en accéléré.
    let dt = real_time.delta_secs();
    let mut direction = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        direction.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        direction.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        direction.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        direction.x += 1.0;
    }
    if direction != Vec2::ZERO {
        let speed = 0.9 * camera.target_distance + 15.0;
        let delta = rotate(direction.normalize(), camera.target_yaw) * speed * dt;
        camera.target_focus += delta;
    }
    if keys.pressed(KeyCode::KeyQ) {
        camera.target_yaw += 1.8 * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        camera.target_yaw -= 1.8 * dt;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        camera.look_at(world::hq_center(&sim));
    }
}

fn mouse_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut camera: Single<&mut RtsCamera>,
) {
    if buttons.any_pressed([MouseButton::Right, MouseButton::Middle]) {
        camera.target_yaw -= motion.delta.x * 0.005;
        camera.target_pitch =
            (camera.target_pitch + motion.delta.y * 0.004).clamp(MIN_PITCH, MAX_PITCH);
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 60.0,
    };
    if notches != 0.0 {
        camera.target_distance =
            (camera.target_distance * 0.88f32.powf(notches)).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
}

/// Un doigt : déplacer. Deux doigts : pincer pour zoomer, glisser verticalement pour incliner.
fn touch_controls(touches: Res<Touches>, mut camera: Single<&mut RtsCamera>) {
    let active: Vec<_> = touches.iter().collect();
    match active.as_slice() {
        [finger] => {
            let scale = camera.target_distance / 600.0;
            let delta = rotate(-finger.delta() * scale, camera.target_yaw);
            camera.target_focus += delta;
        }
        [a, b] => {
            let before = a.previous_position().distance(b.previous_position());
            let after = a.position().distance(b.position());
            if before > 1.0 && after > 1.0 {
                camera.target_distance =
                    (camera.target_distance * before / after).clamp(MIN_DISTANCE, MAX_DISTANCE);
            }
            let vertical = (a.delta().y + b.delta().y) * 0.5;
            camera.target_pitch =
                (camera.target_pitch + vertical * 0.004).clamp(MIN_PITCH, MAX_PITCH);
        }
        _ => {}
    }
}

fn apply_camera(
    real_time: Res<Time<Real>>,
    sim: Res<Sim>,
    camera: Single<(&mut RtsCamera, &mut Transform, &mut DistanceFog)>,
) {
    let (mut camera, mut transform, mut fog) = camera.into_inner();

    // Le point visé reste sur la carte.
    let half = world::half_extent(&sim.map);
    camera.target_focus = camera
        .target_focus
        .clamp(Vec2::splat(-half), Vec2::splat(half));
    camera.target_yaw = camera.target_yaw.rem_euclid(TAU);

    // Rapprochement exponentiel : indépendant de la fréquence d'affichage.
    let t = 1.0 - (-12.0 * real_time.delta_secs()).exp();
    let ground =
        world::ground_height(&sim.map, camera.target_focus).max(world::water_height(&sim.map));
    let target_focus = Vec3::new(camera.target_focus.x, ground, camera.target_focus.y);
    camera.focus = camera.focus.lerp(target_focus, t);
    camera.yaw = lerp_angle(camera.yaw, camera.target_yaw, t);
    camera.pitch += (camera.target_pitch - camera.pitch) * t;
    camera.distance += (camera.target_distance - camera.distance) * t;

    let offset = Quat::from_rotation_y(camera.yaw)
        * Quat::from_rotation_x(-camera.pitch)
        * Vec3::new(0.0, 0.0, camera.distance);
    *transform =
        Transform::from_translation(camera.focus + offset).looking_at(camera.focus, Vec3::Y);

    // Le brouillard suit le zoom : il estompe l'horizon et cache le bord de la carte.
    fog.falloff = FogFalloff::Linear {
        start: camera.distance + 25.0,
        end: camera.distance * 1.3 + 110.0,
    };
}

/// Tourne un déplacement écran (x à droite, y vers le bas) selon l'orientation de la caméra.
fn rotate(v: Vec2, yaw: f32) -> Vec2 {
    let (sin, cos) = yaw.sin_cos();
    Vec2::new(v.x * cos + v.y * sin, -v.x * sin + v.y * cos)
}

fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let diff = (to - from + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
    (from + diff * t).rem_euclid(TAU)
}
