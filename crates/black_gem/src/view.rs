//! Représentation 3D de l'état de la simulation : bâtiments, cristaux en transit,
//! tourelles, ennemis, nids et tirs. Rien ici n'influence la partie.

use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use bevy::{light::NotShadowCaster, prelude::*};
use gem_sim::{
    BuildingKind, BuildingState, Dir,
    building::{CONVEYOR_TICKS, DRILL_PERIOD},
};

use crate::{
    curvature::{CurrentCurvature, CurvedMaterial, bend, curved},
    sim::{NewPlanet, Sim},
    world::{self, PlanetEntity, crystal_mesh},
};

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Views>()
            .add_systems(Startup, create_kit)
            .add_systems(
                Update,
                (
                    reset_views.run_if(on_message::<NewPlanet>),
                    sync_nests,
                    sync_buildings,
                    (
                        animate_drills,
                        animate_conveyor_items,
                        aim_turrets,
                        light_tower_gems,
                    ),
                    sync_enemies,
                    draw_shots,
                )
                    .chain()
                    .after(world::respawn_planet),
            );
    }
}

/// Maillages et matériaux partagés (instanciation automatique : un seul appel de dessin par type).
#[derive(Resource)]
struct Kit {
    meshes: KitMeshes,
    hull: Handle<CurvedMaterial>,
    dark_metal: Handle<CurvedMaterial>,
    belt: Handle<CurvedMaterial>,
    glow_cyan: Handle<CurvedMaterial>,
    glow_purple: Handle<CurvedMaterial>,
    crystal: Handle<CurvedMaterial>,
    gem_charged: Handle<CurvedMaterial>,
    gem_empty: Handle<CurvedMaterial>,
    corruption: Handle<CurvedMaterial>,
    nest: Handle<CurvedMaterial>,
}

struct KitMeshes {
    drill_base: Handle<Mesh>,
    drill_body: Handle<Mesh>,
    drill_rotor: Handle<Mesh>,
    outlet: Handle<Mesh>,
    belt: Handle<Mesh>,
    rail: Handle<Mesh>,
    chevron: Handle<Mesh>,
    crystal: Handle<Mesh>,
    tower_base: Handle<Mesh>,
    tower_column: Handle<Mesh>,
    turret: Handle<Mesh>,
    barrel: Handle<Mesh>,
    enemy: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct Views {
    buildings: HashMap<u32, Entity>,
    enemies: HashMap<u32, Entity>,
    version: Option<u64>,
    nests_seed: Option<u64>,
}

#[derive(Component)]
struct DrillRotor(u32);

#[derive(Component)]
struct ConveyorItem(u32);

#[derive(Component)]
struct Turret(u32);

#[derive(Component)]
struct TowerGem(u32);

#[derive(Component)]
struct EnemyView;

fn create_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CurvedMaterial>>,
) {
    let mut material = |base: StandardMaterial| materials.add(curved(base));
    let glow = |color: Color, emissive: LinearRgba| StandardMaterial {
        base_color: color,
        emissive,
        ..default()
    };
    commands.insert_resource(Kit {
        meshes: KitMeshes {
            drill_base: meshes.add(Cuboid::new(0.86, 0.25, 0.86)),
            drill_body: meshes.add(Cylinder::new(0.28, 0.55).mesh().resolution(8)),
            drill_rotor: meshes.add(Torus::new(0.26, 0.36)),
            outlet: meshes.add(Cuboid::new(0.16, 0.1, 0.3)),
            belt: meshes.add(Cuboid::new(1.0, 0.07, 0.7)),
            rail: meshes.add(Cuboid::new(1.0, 0.12, 0.07)),
            chevron: meshes.add(Cuboid::new(0.26, 0.02, 0.05)),
            crystal: meshes.add(crystal_mesh()),
            tower_base: meshes.add(Cylinder::new(0.46, 0.35).mesh().resolution(6)),
            tower_column: meshes.add(Cylinder::new(0.17, 0.8).mesh().resolution(8)),
            turret: meshes.add(Cuboid::new(0.5, 0.24, 0.36)),
            barrel: meshes.add(Cuboid::new(0.5, 0.08, 0.08)),
            enemy: meshes.add(Sphere::new(0.45).mesh().ico(1).unwrap()),
        },
        hull: material(StandardMaterial {
            base_color: Color::srgb(0.8, 0.82, 0.86),
            metallic: 0.85,
            perceptual_roughness: 0.3,
            ..default()
        }),
        dark_metal: material(StandardMaterial {
            base_color: Color::srgb(0.18, 0.19, 0.22),
            metallic: 0.6,
            perceptual_roughness: 0.5,
            ..default()
        }),
        belt: material(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.1),
            perceptual_roughness: 0.8,
            ..default()
        }),
        glow_cyan: material(glow(
            Color::srgb(0.2, 0.9, 1.0),
            LinearRgba::rgb(0.4, 3.0, 4.0),
        )),
        glow_purple: material(glow(
            Color::srgb(0.6, 0.2, 1.0),
            LinearRgba::rgb(2.5, 0.6, 5.0),
        )),
        crystal: material(StandardMaterial {
            base_color: Color::srgb(0.55, 0.2, 0.95),
            emissive: LinearRgba::rgb(2.4, 0.7, 5.0),
            perceptual_roughness: 0.1,
            ..default()
        }),
        gem_charged: material(glow(
            Color::srgb(0.3, 0.95, 1.0),
            LinearRgba::rgb(1.0, 5.0, 7.0),
        )),
        gem_empty: material(StandardMaterial {
            base_color: Color::srgb(0.15, 0.2, 0.25),
            perceptual_roughness: 0.2,
            ..default()
        }),
        corruption: material(StandardMaterial {
            base_color: Color::srgb(0.05, 0.02, 0.06),
            emissive: LinearRgba::rgb(1.8, 0.12, 1.0),
            metallic: 0.9,
            perceptual_roughness: 0.15,
            ..default()
        }),
        nest: material(StandardMaterial {
            base_color: Color::srgb(0.06, 0.01, 0.03),
            emissive: LinearRgba::rgb(3.0, 0.15, 0.4),
            metallic: 0.8,
            perceptual_roughness: 0.2,
            ..default()
        }),
    });
}

fn reset_views(mut views: ResMut<Views>) {
    // Les entités elles-mêmes sont supprimées avec la planète (`PlanetEntity`).
    *views = Views::default();
}

/// Rotation qui oriente l'axe +X local dans la direction donnée.
fn dir_rotation(dir: Dir) -> Quat {
    let angle = match dir {
        Dir::East => 0.0,
        Dir::North => FRAC_PI_2,
        Dir::West => std::f32::consts::PI,
        Dir::South => -FRAC_PI_2,
    };
    Quat::from_rotation_y(angle)
}

fn sync_nests(mut commands: Commands, sim: Res<Sim>, kit: Res<Kit>, mut views: ResMut<Views>) {
    if views.nests_seed == Some(sim.map.seed) {
        return;
    }
    views.nests_seed = Some(sim.map.seed);
    for (i, &nest) in sim.nests.iter().enumerate() {
        let center = world::tile_to_world(&sim.map, nest);
        commands
            .spawn((
                PlanetEntity,
                Transform::from_translation(center),
                Visibility::default(),
            ))
            .with_children(|parent| {
                for j in 0..7 {
                    let angle = j as f32 * 0.9 + i as f32;
                    let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * (0.3 + 0.12 * j as f32);
                    let tilt =
                        Quat::from_axis_angle(Vec3::new(angle.sin(), 0.0, -angle.cos()), 0.35);
                    parent.spawn((
                        Mesh3d(kit.meshes.crystal.clone()),
                        MeshMaterial3d(kit.nest.clone()),
                        Transform::from_translation(offset)
                            .with_rotation(tilt)
                            .with_scale(Vec3::splat(1.6 + 0.25 * (j % 3) as f32)),
                    ));
                }
                parent.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.2, 0.4),
                        intensity: 40_000.0,
                        range: 14.0,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 2.5, 0.0),
                ));
            });
    }
}

fn sync_buildings(mut commands: Commands, sim: Res<Sim>, kit: Res<Kit>, mut views: ResMut<Views>) {
    if views.version == Some(sim.buildings.version) {
        return;
    }
    views.version = Some(sim.buildings.version);

    views.buildings.retain(|id, entity| {
        let alive = sim.buildings.get(*id).is_some();
        if !alive {
            commands.entity(*entity).despawn();
        }
        alive
    });

    for building in sim.buildings.iter() {
        // Le QG est déjà représenté par le décor de la planète.
        if building.kind == BuildingKind::Hq || views.buildings.contains_key(&building.id) {
            continue;
        }
        let center = world::pos_to_world(&sim.map, building.center());
        let rotation = if building.kind == BuildingKind::Tower {
            Quat::IDENTITY
        } else {
            dir_rotation(building.dir)
        };
        let id = building.id;
        let m = &kit.meshes;
        let mut root = commands.spawn((
            PlanetEntity,
            Transform::from_translation(center).with_rotation(rotation),
            Visibility::default(),
        ));
        root.with_children(|parent| match building.kind {
            BuildingKind::Drill => {
                parent.spawn((
                    Mesh3d(m.drill_base.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.12, 0.0),
                ));
                parent.spawn((
                    Mesh3d(m.drill_body.clone()),
                    MeshMaterial3d(kit.hull.clone()),
                    Transform::from_xyz(0.0, 0.5, 0.0),
                ));
                parent.spawn((
                    DrillRotor(id),
                    Mesh3d(m.drill_rotor.clone()),
                    MeshMaterial3d(kit.glow_purple.clone()),
                    Transform::from_xyz(0.0, 0.45, 0.0),
                ));
                parent.spawn((
                    Mesh3d(m.outlet.clone()),
                    MeshMaterial3d(kit.glow_cyan.clone()),
                    Transform::from_xyz(0.42, 0.18, 0.0),
                ));
            }
            BuildingKind::Conveyor => {
                parent.spawn((
                    Mesh3d(m.belt.clone()),
                    MeshMaterial3d(kit.belt.clone()),
                    Transform::from_xyz(0.0, 0.035, 0.0),
                ));
                for side in [-0.38, 0.38] {
                    parent.spawn((
                        Mesh3d(m.rail.clone()),
                        MeshMaterial3d(kit.hull.clone()),
                        Transform::from_xyz(0.0, 0.06, side),
                    ));
                }
                // Chevron « > » qui indique le sens de marche.
                for side in [-1.0f32, 1.0] {
                    parent.spawn((
                        Mesh3d(m.chevron.clone()),
                        MeshMaterial3d(kit.glow_cyan.clone()),
                        Transform::from_xyz(0.0, 0.075, side * 0.07)
                            .with_rotation(Quat::from_rotation_y(side * 0.6)),
                        NotShadowCaster,
                    ));
                }
                parent.spawn((
                    ConveyorItem(id),
                    Mesh3d(m.crystal.clone()),
                    MeshMaterial3d(kit.crystal.clone()),
                    Transform::from_xyz(0.0, 0.07, 0.0).with_scale(Vec3::splat(0.3)),
                    Visibility::Hidden,
                ));
            }
            BuildingKind::Tower => {
                parent.spawn((
                    Mesh3d(m.tower_base.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.17, 0.0),
                ));
                parent.spawn((
                    Mesh3d(m.tower_column.clone()),
                    MeshMaterial3d(kit.hull.clone()),
                    Transform::from_xyz(0.0, 0.7, 0.0),
                ));
                parent
                    .spawn((
                        Turret(id),
                        Transform::from_xyz(0.0, 1.15, 0.0),
                        Visibility::default(),
                    ))
                    .with_children(|turret| {
                        turret.spawn((
                            Mesh3d(m.turret.clone()),
                            MeshMaterial3d(kit.hull.clone()),
                            Transform::default(),
                        ));
                        turret.spawn((
                            Mesh3d(m.barrel.clone()),
                            MeshMaterial3d(kit.dark_metal.clone()),
                            Transform::from_xyz(0.35, 0.02, 0.0),
                        ));
                        turret.spawn((
                            TowerGem(id),
                            Mesh3d(m.crystal.clone()),
                            MeshMaterial3d(kit.gem_empty.clone()),
                            Transform::from_xyz(-0.05, 0.1, 0.0).with_scale(Vec3::splat(0.38)),
                        ));
                    });
            }
            BuildingKind::Hq => {}
        });
        views.buildings.insert(id, root.id());
    }
}

fn animate_drills(
    sim: Res<Sim>,
    time: Res<Time>,
    mut rotors: Query<(&DrillRotor, &mut Transform)>,
) {
    for (rotor, mut transform) in &mut rotors {
        if let Some(BuildingState::Drill { buffer, timer }) =
            sim.buildings.get(rotor.0).map(|b| &b.state)
        {
            // La foreuse tourne tant qu'elle produit, et s'arrête quand sa sortie est bouchée.
            let working = *buffer < 4;
            if working {
                transform.rotate_y(time.delta_secs() * 6.0);
            }
            let pulse = (*timer as f32 / DRILL_PERIOD as f32 * std::f32::consts::TAU).sin();
            transform.translation.y = 0.45 + if working { 0.04 * pulse } else { 0.0 };
        }
    }
}

/// Cristaux sur les convoyeurs, interpolés entre deux ticks pour un mouvement fluide.
fn animate_conveyor_items(
    sim: Res<Sim>,
    fixed: Res<Time<Fixed>>,
    mut items: Query<(&ConveyorItem, &mut Transform, &mut Visibility)>,
) {
    let alpha = fixed.overstep_fraction();
    for (item, mut transform, mut visibility) in &mut items {
        match sim.buildings.get(item.0).map(|b| &b.state) {
            Some(BuildingState::Conveyor {
                item: Some(progress),
            }) => {
                let moving = (*progress < CONVEYOR_TICKS) as u32 as f32;
                let t = ((*progress as f32 + alpha * moving) / CONVEYOR_TICKS as f32).min(1.0);
                transform.translation.x = -0.5 + t;
                *visibility = Visibility::Inherited;
            }
            _ => *visibility = Visibility::Hidden,
        }
    }
}

/// Les tourelles pivotent vers leur dernière cible.
fn aim_turrets(sim: Res<Sim>, mut turrets: Query<(&Turret, &mut Transform)>) {
    for (turret, mut transform) in &mut turrets {
        let Some(building) = sim.buildings.get(turret.0) else {
            continue;
        };
        let center = building.center();
        if let Some(shot) = sim.shots.iter().rev().find(|s| s.from == center) {
            let from = world::pos_to_world(&sim.map, shot.from);
            let to = world::pos_to_world(&sim.map, shot.to);
            let d = to - from;
            let target = Quat::from_rotation_y((-d.z).atan2(d.x));
            transform.rotation = transform.rotation.slerp(target, 0.5);
        }
    }
}

/// La gemme d'une tour brille quand elle a des munitions.
fn light_tower_gems(
    sim: Res<Sim>,
    kit: Res<Kit>,
    mut gems: Query<(&TowerGem, &mut MeshMaterial3d<CurvedMaterial>)>,
) {
    for (gem, mut material) in &mut gems {
        if let Some(BuildingState::Tower {
            crystals, charge, ..
        }) = sim.buildings.get(gem.0).map(|b| &b.state)
        {
            let wanted = if *crystals + *charge > 0 {
                &kit.gem_charged
            } else {
                &kit.gem_empty
            };
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
    }
}

fn sync_enemies(
    mut commands: Commands,
    sim: Res<Sim>,
    kit: Res<Kit>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    mut views: ResMut<Views>,
    mut transforms: Query<&mut Transform, With<EnemyView>>,
) {
    views.enemies.retain(|id, entity| {
        let alive = sim.enemies.iter().any(|e| e.id == *id);
        if !alive {
            commands.entity(*entity).despawn();
        }
        alive
    });

    let alpha = fixed.overstep_fraction();
    let t = time.elapsed_secs();
    for enemy in &sim.enemies {
        let from = world::pos_to_world(&sim.map, enemy.prev_pos);
        let to = world::pos_to_world(&sim.map, enemy.pos);
        let bob = 0.35 + 0.08 * (t * 6.0 + enemy.id as f32).sin();
        let scale = 0.65 + 0.35 * enemy.hp.max(0) as f32 / enemy.max_hp as f32;
        let transform = Transform::from_translation(from.lerp(to, alpha) + Vec3::Y * bob)
            .with_rotation(Quat::from_rotation_y(t * 1.5 + enemy.id as f32))
            .with_scale(Vec3::new(1.0, 0.8, 1.0) * scale);
        match views.enemies.get(&enemy.id) {
            Some(&entity) => {
                if let Ok(mut current) = transforms.get_mut(entity) {
                    *current = transform;
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        PlanetEntity,
                        EnemyView,
                        Mesh3d(kit.meshes.enemy.clone()),
                        MeshMaterial3d(kit.corruption.clone()),
                        transform,
                    ))
                    .id();
                views.enemies.insert(enemy.id, entity);
            }
        }
    }
}

/// Durée d'affichage d'un rayon de tir, en secondes réelles.
const BEAM_SECONDS: f32 = 0.18;

/// Rayons de tir : lignes lumineuses (HDR, donc avec halo) qui s'estompent.
/// Le fondu se fait en temps réel : même en vitesse ×20, chaque tir reste visible.
fn draw_shots(
    mut gizmos: Gizmos,
    sim: Res<Sim>,
    time: Res<Time<Real>>,
    curvature: Res<CurrentCurvature>,
    mut beams: Local<Vec<(gem_sim::Shot, f32)>>,
    mut last_tick: Local<Option<u64>>,
) {
    let now = time.elapsed_secs();
    let seen = last_tick.unwrap_or(0);
    let fresh = sim
        .shots
        .iter()
        .filter(|s| last_tick.is_none() || s.tick > seen)
        .map(|s| (*s, now));
    beams.extend(fresh);
    *last_tick = sim.shots.iter().map(|s| s.tick).max().or(*last_tick);
    beams.retain(|(_, born)| now - born < BEAM_SECONDS);
    // Nouvelle planète : le temps de la simulation repart en arrière.
    if last_tick.is_some_and(|t| t > sim.tick) {
        *last_tick = None;
        beams.clear();
    }

    for (shot, born) in beams.iter() {
        let fade = 1.0 - (now - born) / BEAM_SECONDS;
        let (height, color) = if shot.from_hq {
            (3.0, LinearRgba::rgb(3.0, 3.0, 4.0))
        } else {
            (1.25, LinearRgba::rgb(0.6, 4.0, 6.0))
        };
        let from = world::pos_to_world(&sim.map, shot.from) + Vec3::Y * height;
        let to = world::pos_to_world(&sim.map, shot.to) + Vec3::Y * 0.4;
        gizmos.line(
            bend(from, curvature.params),
            bend(to, curvature.params),
            color * fade,
        );
    }
}
