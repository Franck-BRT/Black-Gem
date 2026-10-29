//! Représentation 3D de l'état de la simulation : bâtiments, cristaux en transit,
//! tourelles, ennemis, nids et tirs. Rien ici n'influence la partie.

use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use bevy::{light::NotShadowCaster, prelude::*};
use gem_sim::{
    BuildingKind, BuildingState, Dir, Gem, GemColor, Item, ShotSource,
    building::{CONVEYOR_TICKS, DRILL_PERIOD, FUSE_TICKS},
};

use crate::{
    curvature::{CurrentCurvature, CurvedMaterial, bend, curved},
    palette::{gem_glow, gem_rgb},
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
                    sync_core_spire,
                    sync_buildings,
                    (
                        animate_drills,
                        animate_conveyor_items,
                        animate_cutters,
                        animate_furnaces,
                        animate_core_drill,
                        aim_turrets,
                        light_tower_gems,
                    ),
                    sync_enemies,
                    sync_golems,
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
    glow_white: Handle<CurvedMaterial>,
    /// Cristal brut de chaque couleur.
    crystals: [Handle<CurvedMaterial>; 5],
    /// Gemme taillée de chaque couleur : brillante, et éteinte (tour sans munitions).
    gems: [Handle<CurvedMaterial>; 5],
    gems_dim: [Handle<CurvedMaterial>; 5],
    gem_charged: Handle<CurvedMaterial>,
    gem_empty: Handle<CurvedMaterial>,
    furnace_idle: Handle<CurvedMaterial>,
    golem_error: Handle<CurvedMaterial>,
    black_gem: Handle<CurvedMaterial>,
    corruption_boss: Handle<CurvedMaterial>,
    golem_slow: Handle<CurvedMaterial>,
    corruption: Handle<CurvedMaterial>,
    corruption_poisoned: Handle<CurvedMaterial>,
    corruption_slowed: Handle<CurvedMaterial>,
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
    gem: Handle<Mesh>,
    cutter_body: Handle<Mesh>,
    saw: Handle<Mesh>,
    furnace_body: Handle<Mesh>,
    furnace_core: Handle<Mesh>,
    tower_base: Handle<Mesh>,
    tower_column: Handle<Mesh>,
    turret: Handle<Mesh>,
    barrel: Handle<Mesh>,
    enemy: Handle<Mesh>,
    golem_body: Handle<Mesh>,
    golem_ring: Handle<Mesh>,
    core_base: Handle<Mesh>,
    pylon: Handle<Mesh>,
    frame: Handle<Mesh>,
    shaft: Handle<Mesh>,
    core_ring: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct Views {
    buildings: HashMap<u32, Entity>,
    enemies: HashMap<u32, Entity>,
    golems: HashMap<u32, Entity>,
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

#[derive(Component)]
struct CutterSaw(u32);

#[derive(Component)]
struct GolemRing(u32);

#[derive(Component)]
struct CoreShaft(u32);

#[derive(Component)]
struct CoreRing;

/// La Gemme Noire, qui sort de terre à l'extraction.
#[derive(Component)]
struct BlackGemView;

/// La flèche d'obsidienne qui marque le Cœur, cachée une fois le Grand foreur construit.
#[derive(Component)]
struct CoreSpire;

#[derive(Component)]
struct GolemCargo(u32);

#[derive(Component)]
struct FurnaceCore(u32);

/// Gemme taillée : table, couronne et pavillon à facettes.
fn gem_mesh() -> Mesh {
    const SIDES: usize = 8;
    let ring = |y: f32, r: f32, twist: f32| -> Vec<Vec3> {
        (0..SIDES)
            .map(|i| {
                let a = (i as f32 + twist) / SIDES as f32 * std::f32::consts::TAU;
                Vec3::new(a.cos() * r, y, a.sin() * r)
            })
            .collect()
    };
    let table = ring(0.2, 0.24, 0.5);
    let girdle = ring(0.0, 0.42, 0.0);
    let top = Vec3::new(0.0, 0.2, 0.0);
    let bottom = Vec3::new(0.0, -0.42, 0.0);
    let mut triangles = Vec::new();
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        triangles.push([top, table[j], table[i]]);
        triangles.push([table[i], table[j], girdle[j]]);
        triangles.push([table[i], girdle[j], girdle[i]]);
        triangles.push([girdle[i], girdle[j], bottom]);
    }
    world::flat_mesh(&triangles)
}

fn create_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CurvedMaterial>>,
) {
    let mut material = |base: StandardMaterial| materials.add(curved(base));
    let corruption_base = |emissive: LinearRgba| StandardMaterial {
        base_color: Color::srgb(0.05, 0.02, 0.06),
        emissive,
        metallic: 0.9,
        perceptual_roughness: 0.15,
        ..default()
    };
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
            gem: meshes.add(gem_mesh()),
            cutter_body: meshes.add(Cuboid::new(0.7, 0.5, 0.6)),
            saw: meshes.add(Cylinder::new(0.3, 0.04).mesh().resolution(12)),
            furnace_body: meshes.add(Cylinder::new(0.4, 0.7).mesh().resolution(8)),
            furnace_core: meshes.add(Sphere::new(0.22).mesh().ico(2).unwrap()),
            tower_base: meshes.add(Cylinder::new(0.46, 0.35).mesh().resolution(6)),
            tower_column: meshes.add(Cylinder::new(0.17, 0.8).mesh().resolution(8)),
            turret: meshes.add(Cuboid::new(0.5, 0.24, 0.36)),
            barrel: meshes.add(Cuboid::new(0.5, 0.08, 0.08)),
            enemy: meshes.add(Sphere::new(0.45).mesh().ico(1).unwrap()),
            golem_body: meshes.add(Sphere::new(0.38).mesh().ico(2).unwrap()),
            golem_ring: meshes.add(Torus::new(0.5, 0.6)),
            core_base: meshes.add(Cuboid::new(2.9, 0.3, 2.9)),
            pylon: meshes.add(Cuboid::new(0.28, 3.8, 0.28)),
            frame: meshes.add(Cuboid::new(2.2, 0.25, 2.2)),
            shaft: meshes.add(Cylinder::new(0.32, 3.6).mesh().resolution(8)),
            core_ring: meshes.add(Torus::new(0.9, 1.15)),
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
        glow_white: material(glow(
            Color::srgb(0.9, 0.9, 1.0),
            LinearRgba::rgb(2.5, 2.5, 3.0),
        )),
        crystals: GemColor::ALL.map(|color| {
            let tint = gem_rgb(color);
            material(StandardMaterial {
                base_color: Color::srgb(tint.x, tint.y, tint.z),
                emissive: gem_glow(color, 2.5),
                perceptual_roughness: 0.1,
                ..default()
            })
        }),
        gems: GemColor::ALL.map(|color| {
            let tint = gem_rgb(color);
            material(StandardMaterial {
                base_color: Color::srgb(tint.x, tint.y, tint.z),
                emissive: gem_glow(color, 6.0),
                perceptual_roughness: 0.05,
                reflectance: 1.0,
                ..default()
            })
        }),
        gems_dim: GemColor::ALL.map(|color| {
            let tint = gem_rgb(color) * 0.4;
            material(StandardMaterial {
                base_color: Color::srgb(tint.x, tint.y, tint.z),
                emissive: gem_glow(color, 0.3),
                perceptual_roughness: 0.15,
                ..default()
            })
        }),
        black_gem: material(StandardMaterial {
            base_color: Color::srgb(0.01, 0.0, 0.02),
            emissive: LinearRgba::rgb(0.45, 0.04, 1.1),
            metallic: 0.9,
            perceptual_roughness: 0.05,
            reflectance: 1.0,
            ..default()
        }),
        corruption_boss: material(corruption_base(LinearRgba::rgb(4.0, 0.2, 0.3))),
        golem_error: material(glow(
            Color::srgb(1.0, 0.2, 0.2),
            LinearRgba::rgb(5.0, 0.4, 0.3),
        )),
        golem_slow: material(glow(
            Color::srgb(1.0, 0.85, 0.2),
            LinearRgba::rgb(4.0, 3.0, 0.4),
        )),
        furnace_idle: material(glow(
            Color::srgb(0.4, 0.15, 0.05),
            LinearRgba::rgb(0.8, 0.25, 0.05),
        )),
        gem_charged: material(glow(
            Color::srgb(0.3, 0.95, 1.0),
            LinearRgba::rgb(1.0, 5.0, 7.0),
        )),
        gem_empty: material(StandardMaterial {
            base_color: Color::srgb(0.15, 0.2, 0.25),
            perceptual_roughness: 0.2,
            ..default()
        }),
        corruption: material(corruption_base(LinearRgba::rgb(1.8, 0.12, 1.0))),
        corruption_poisoned: material(corruption_base(LinearRgba::rgb(0.3, 2.2, 0.4))),
        corruption_slowed: material(corruption_base(LinearRgba::rgb(0.2, 1.4, 2.6))),
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
                let rotor = match building.state {
                    BuildingState::Drill { color, .. } => kit.crystals[color.index()].clone(),
                    _ => kit.glow_purple.clone(),
                };
                parent.spawn((
                    DrillRotor(id),
                    Mesh3d(m.drill_rotor.clone()),
                    MeshMaterial3d(rotor),
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
                    MeshMaterial3d(kit.crystals[0].clone()),
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
            BuildingKind::Cutter => {
                parent.spawn((
                    Mesh3d(m.drill_base.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.12, 0.0),
                ));
                parent.spawn((
                    Mesh3d(m.cutter_body.clone()),
                    MeshMaterial3d(kit.hull.clone()),
                    Transform::from_xyz(-0.08, 0.45, 0.0),
                ));
                // Disque de taille, vertical, qui tourne pendant la taille.
                parent.spawn((
                    CutterSaw(id),
                    Mesh3d(m.saw.clone()),
                    MeshMaterial3d(kit.glow_white.clone()),
                    Transform::from_xyz(0.05, 0.62, 0.32)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
                ));
                parent.spawn((
                    Mesh3d(m.outlet.clone()),
                    MeshMaterial3d(kit.glow_cyan.clone()),
                    Transform::from_xyz(0.42, 0.18, 0.0),
                ));
            }
            BuildingKind::Furnace => {
                parent.spawn((
                    Mesh3d(m.drill_base.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.12, 0.0),
                ));
                parent.spawn((
                    Mesh3d(m.furnace_body.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.55, 0.0),
                ));
                parent.spawn((
                    FurnaceCore(id),
                    Mesh3d(m.furnace_core.clone()),
                    MeshMaterial3d(kit.furnace_idle.clone()),
                    Transform::from_xyz(0.0, 0.95, 0.0),
                    NotShadowCaster,
                ));
                parent.spawn((
                    Mesh3d(m.outlet.clone()),
                    MeshMaterial3d(kit.glow_cyan.clone()),
                    Transform::from_xyz(0.42, 0.18, 0.0),
                ));
            }
            BuildingKind::CoreDrill => {
                parent.spawn((
                    Mesh3d(m.core_base.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 0.15, 0.0),
                ));
                // Derrick : quatre pylônes inclinés vers un cadre au sommet.
                for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    parent.spawn((
                        Mesh3d(m.pylon.clone()),
                        MeshMaterial3d(kit.hull.clone()),
                        Transform::from_xyz(sx * 1.05, 2.0, sz * 1.05).with_rotation(
                            Quat::from_rotation_z(sx * 0.12) * Quat::from_rotation_x(-sz * 0.12),
                        ),
                    ));
                }
                parent.spawn((
                    Mesh3d(m.frame.clone()),
                    MeshMaterial3d(kit.hull.clone()),
                    Transform::from_xyz(0.0, 3.85, 0.0),
                ));
                parent.spawn((
                    CoreShaft(id),
                    Mesh3d(m.shaft.clone()),
                    MeshMaterial3d(kit.dark_metal.clone()),
                    Transform::from_xyz(0.0, 2.0, 0.0),
                ));
                parent.spawn((
                    CoreRing,
                    Mesh3d(m.core_ring.clone()),
                    MeshMaterial3d(kit.glow_purple.clone()),
                    Transform::from_xyz(0.0, 0.45, 0.0),
                    NotShadowCaster,
                ));
                parent.spawn((
                    BlackGemView,
                    Mesh3d(m.gem.clone()),
                    MeshMaterial3d(kit.black_gem.clone()),
                    Transform::from_xyz(0.0, 5.5, 0.0).with_scale(Vec3::splat(2.2)),
                    Visibility::Hidden,
                ));
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
        if let Some(BuildingState::Drill { buffer, timer, .. }) =
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
/// Apparence d'un objet : maillage, matériau et taille.
fn item_look(kit: &Kit, item: Item) -> (Handle<Mesh>, Handle<CurvedMaterial>, f32) {
    match item {
        Item::Crystal(color) => (
            kit.meshes.crystal.clone(),
            kit.crystals[color.index()].clone(),
            0.3,
        ),
        Item::Gem(gem) => (
            kit.meshes.gem.clone(),
            kit.gems[gem.main_color().index()].clone(),
            gem_scale(gem),
        ),
        Item::Shard => (kit.meshes.crystal.clone(), kit.glow_white.clone(), 0.25),
    }
}

fn gem_scale(gem: Gem) -> f32 {
    0.32 + 0.05 * gem.grade as f32
}

/// Objets sur les convoyeurs, interpolés entre deux ticks pour un mouvement fluide.
#[allow(clippy::type_complexity)]
fn animate_conveyor_items(
    sim: Res<Sim>,
    kit: Res<Kit>,
    fixed: Res<Time<Fixed>>,
    mut items: Query<(
        &ConveyorItem,
        &mut Transform,
        &mut Visibility,
        &mut Mesh3d,
        &mut MeshMaterial3d<CurvedMaterial>,
    )>,
) {
    let alpha = fixed.overstep_fraction();
    for (item, mut transform, mut visibility, mut mesh, mut material) in &mut items {
        match sim.buildings.get(item.0).map(|b| &b.state) {
            Some(BuildingState::Conveyor {
                item: Some((carried, progress)),
            }) => {
                let moving = (*progress < CONVEYOR_TICKS) as u32 as f32;
                let t = ((*progress as f32 + alpha * moving) / CONVEYOR_TICKS as f32).min(1.0);
                let (new_mesh, new_material, scale) = item_look(&kit, *carried);
                if mesh.0 != new_mesh {
                    mesh.0 = new_mesh;
                }
                if material.0 != new_material {
                    material.0 = new_material;
                }
                // Une gemme est posée sur sa pointe : on la relève un peu.
                let lift = if matches!(carried, Item::Gem(_)) {
                    0.42 * scale
                } else {
                    0.0
                };
                transform.translation = Vec3::new(-0.5 + t, 0.07 + lift, 0.0);
                transform.scale = Vec3::splat(scale);
                *visibility = Visibility::Inherited;
            }
            _ => *visibility = Visibility::Hidden,
        }
    }
}

/// Le disque de la taillerie tourne pendant la taille.
fn animate_cutters(sim: Res<Sim>, time: Res<Time>, mut saws: Query<(&CutterSaw, &mut Transform)>) {
    for (saw, mut transform) in &mut saws {
        if let Some(BuildingState::Cutter {
            cutting: Some(_), ..
        }) = sim.buildings.get(saw.0).map(|b| &b.state)
        {
            transform.rotate_local_y(time.delta_secs() * 14.0);
        }
    }
}

/// Le cœur du four grossit et prend la couleur des gemmes pendant la fusion.
fn animate_furnaces(
    sim: Res<Sim>,
    kit: Res<Kit>,
    time: Res<Time>,
    mut cores: Query<(
        &FurnaceCore,
        &mut Transform,
        &mut MeshMaterial3d<CurvedMaterial>,
    )>,
) {
    for (core, mut transform, mut material) in &mut cores {
        let Some(BuildingState::Furnace {
            slots, progress, ..
        }) = sim.buildings.get(core.0).map(|b| &b.state)
        else {
            continue;
        };
        let (wanted, size) = match slots {
            [Some(gem), _] | [None, Some(gem)] => {
                let heat = *progress as f32 / FUSE_TICKS as f32;
                let pulse = 0.08 * (time.elapsed_secs() * (4.0 + 10.0 * heat)).sin();
                (
                    kit.gems[gem.main_color().index()].clone(),
                    1.0 + 0.6 * heat + pulse,
                )
            }
            [None, None] => (kit.furnace_idle.clone(), 0.8),
        };
        if material.0 != wanted {
            material.0 = wanted;
        }
        transform.scale = Vec3::splat(size);
    }
}

/// Les tourelles pivotent vers leur dernière cible.
fn aim_turrets(sim: Res<Sim>, mut turrets: Query<(&Turret, &mut Transform)>) {
    for (turret, mut transform) in &mut turrets {
        let Some(building) = sim.buildings.get(turret.0) else {
            continue;
        };
        let center = building.center();
        if let Some(shot) = sim
            .shots
            .iter()
            .rev()
            .find(|s| s.source == ShotSource::Tower && s.from == center)
        {
            let from = world::pos_to_world(&sim.map, shot.from);
            let to = world::pos_to_world(&sim.map, shot.to);
            let d = to - from;
            let target = Quat::from_rotation_y((-d.z).atan2(d.x));
            transform.rotation = transform.rotation.slerp(target, 0.5);
        }
    }
}

/// La gemme d'une tour brille quand elle a des munitions.
/// La tour montre sa gemme sertie (taille selon le grade) ; elle brille quand la tour a
/// des munitions. Sans gemme : un simple éclat de cristal.
fn light_tower_gems(
    sim: Res<Sim>,
    kit: Res<Kit>,
    mut gems: Query<(
        &TowerGem,
        &mut Transform,
        &mut Mesh3d,
        &mut MeshMaterial3d<CurvedMaterial>,
    )>,
) {
    for (tower_gem, mut transform, mut mesh, mut material) in &mut gems {
        let Some(BuildingState::Tower {
            crystals,
            charge,
            gem,
            ..
        }) = sim.buildings.get(tower_gem.0).map(|b| &b.state)
        else {
            continue;
        };
        let armed = *crystals + *charge > 0;
        let (new_mesh, new_material, scale, y) = match gem {
            Some(gem) => {
                let color = gem.main_color().index();
                let material = if armed {
                    &kit.gems[color]
                } else {
                    &kit.gems_dim[color]
                };
                let scale = gem_scale(*gem) * 1.4;
                (
                    kit.meshes.gem.clone(),
                    material.clone(),
                    scale,
                    0.15 + 0.42 * scale,
                )
            }
            None => {
                let material = if armed {
                    &kit.gem_charged
                } else {
                    &kit.gem_empty
                };
                (kit.meshes.crystal.clone(), material.clone(), 0.38, 0.1)
            }
        };
        if mesh.0 != new_mesh {
            mesh.0 = new_mesh;
        }
        if material.0 != new_material {
            material.0 = new_material;
        }
        transform.scale = Vec3::splat(scale);
        transform.translation.y = y;
    }
}

fn sync_enemies(
    mut commands: Commands,
    sim: Res<Sim>,
    kit: Res<Kit>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    mut views: ResMut<Views>,
    mut transforms: Query<(&mut Transform, &mut MeshMaterial3d<CurvedMaterial>), With<EnemyView>>,
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
        let scale = (0.65 + 0.35 * enemy.hp.max(0) as f32 / enemy.max_hp as f32)
            * if enemy.boss { 2.2 } else { 1.0 };
        let transform = Transform::from_translation(from.lerp(to, alpha) + Vec3::Y * bob)
            .with_rotation(Quat::from_rotation_y(t * 1.5 + enemy.id as f32))
            .with_scale(Vec3::new(1.0, 0.8, 1.0) * scale);
        match views.enemies.get(&enemy.id) {
            Some(&entity) => {
                if let Ok((mut current, mut material)) = transforms.get_mut(entity) {
                    *current = transform;
                    // Teinte selon l'effet subi : vert empoisonné, cyan ralenti.
                    let wanted = if enemy.boss {
                        &kit.corruption_boss
                    } else if enemy.poison_ticks > 0 {
                        &kit.corruption_poisoned
                    } else if enemy.slow_ticks > 0 {
                        &kit.corruption_slowed
                    } else {
                        &kit.corruption
                    };
                    if material.0 != *wanted {
                        material.0 = wanted.clone();
                    }
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
        let color = match (shot.source, shot.color) {
            (ShotSource::Hq, _) => LinearRgba::rgb(3.0, 3.0, 4.0),
            (_, Some(gem)) => gem_glow(gem, 6.0),
            (_, None) => LinearRgba::rgb(0.6, 4.0, 6.0),
        };
        let height = match shot.source {
            ShotSource::Hq => 3.0,
            ShotSource::Tower => 1.35,
            ShotSource::Chain => 0.4,
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

/// Golems : un drone qui flotte, un anneau qui tourne (cyan en marche, jaune en mode lent,
/// rouge en erreur, gris à l'arrêt) et l'objet transporté en dessous.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_golems(
    mut commands: Commands,
    sim: Res<Sim>,
    kit: Res<Kit>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    mut views: ResMut<Views>,
    mut roots: Query<&mut Transform, (Without<GolemRing>, Without<GolemCargo>)>,
    mut rings: Query<
        (
            &GolemRing,
            &mut Transform,
            &mut MeshMaterial3d<CurvedMaterial>,
        ),
        Without<GolemCargo>,
    >,
    mut cargos: Query<
        (
            &GolemCargo,
            &mut Transform,
            &mut Visibility,
            &mut Mesh3d,
            &mut MeshMaterial3d<CurvedMaterial>,
        ),
        Without<GolemRing>,
    >,
) {
    views.golems.retain(|id, entity| {
        let alive = sim.golem(*id).is_some();
        if !alive {
            commands.entity(*entity).despawn();
        }
        alive
    });

    let alpha = fixed.overstep_fraction();
    let t = time.elapsed_secs();
    for golem in &sim.golems {
        let from = world::pos_to_world(&sim.map, golem.prev_pos);
        let to = world::pos_to_world(&sim.map, golem.pos);
        let hover = 1.5 + 0.12 * (t * 2.5 + golem.id as f32).sin();
        let transform = Transform::from_translation(from.lerp(to, alpha) + Vec3::Y * hover);
        match views.golems.get(&golem.id) {
            Some(&entity) => {
                if let Ok(mut current) = roots.get_mut(entity) {
                    *current = transform;
                }
            }
            None => {
                let id = golem.id;
                let entity = commands
                    .spawn((PlanetEntity, transform, Visibility::default()))
                    .with_children(|parent| {
                        parent.spawn((
                            Mesh3d(kit.meshes.golem_body.clone()),
                            MeshMaterial3d(kit.hull.clone()),
                            Transform::default(),
                        ));
                        parent.spawn((
                            GolemRing(id),
                            Mesh3d(kit.meshes.golem_ring.clone()),
                            MeshMaterial3d(kit.glow_cyan.clone()),
                            Transform::default(),
                            NotShadowCaster,
                        ));
                        parent.spawn((
                            GolemCargo(id),
                            Mesh3d(kit.meshes.crystal.clone()),
                            MeshMaterial3d(kit.glow_white.clone()),
                            Transform::from_xyz(0.0, -0.7, 0.0),
                            Visibility::Hidden,
                        ));
                    })
                    .id();
                views.golems.insert(id, entity);
            }
        }
    }

    for (ring, mut transform, mut material) in &mut rings {
        let Some(golem) = sim.golem(ring.0) else {
            continue;
        };
        let (wanted, spin) = match golem.status {
            gem_sim::GolemStatus::Error(_) => (&kit.golem_error, 0.0),
            gem_sim::GolemStatus::Running if golem.slow => (&kit.golem_slow, 1.0),
            gem_sim::GolemStatus::Running => (&kit.glow_cyan, 3.0),
            _ => (&kit.dark_metal, 0.0),
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
        transform.rotation = Quat::from_rotation_y(t * spin) * Quat::from_rotation_x(0.35);
    }

    for (cargo, mut transform, mut visibility, mut mesh, mut material) in &mut cargos {
        match sim.golem(cargo.0).and_then(|g| g.carrying) {
            Some(item) => {
                let (new_mesh, new_material, scale) = item_look(&kit, item);
                if mesh.0 != new_mesh {
                    mesh.0 = new_mesh;
                }
                if material.0 != new_material {
                    material.0 = new_material;
                }
                transform.scale = Vec3::splat(scale);
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

/// La flèche d'obsidienne du Cœur : de grands cristaux noirs qui luisent de violet.
fn sync_core_spire(
    mut commands: Commands,
    sim: Res<Sim>,
    kit: Res<Kit>,
    mut spawned_for: Local<Option<u64>>,
    mut spires: Query<&mut Visibility, With<CoreSpire>>,
) {
    if *spawned_for != Some(sim.map.seed ^ sim.map.size as u64) {
        *spawned_for = Some(sim.map.seed ^ sim.map.size as u64);
        if let Some(core) = sim.core {
            let center = world::tile_to_world(&sim.map, gem_sim::Tile::new(core.x + 1, core.y + 1));
            commands
                .spawn((
                    PlanetEntity,
                    CoreSpire,
                    Transform::from_translation(center),
                    Visibility::default(),
                ))
                .with_children(|parent| {
                    for (i, (x, z, scale, tilt)) in [
                        (0.0, 0.0, 3.2, 0.0),
                        (0.7, 0.3, 1.8, 0.35),
                        (-0.6, 0.5, 1.5, -0.4),
                        (0.2, -0.7, 1.6, 0.3),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        parent.spawn((
                            Mesh3d(kit.meshes.crystal.clone()),
                            MeshMaterial3d(kit.black_gem.clone()),
                            Transform::from_xyz(x, -0.1, z)
                                .with_rotation(
                                    Quat::from_rotation_y(i as f32 * 1.7)
                                        * Quat::from_rotation_x(tilt),
                                )
                                .with_scale(Vec3::splat(scale)),
                        ));
                    }
                    parent.spawn((
                        PointLight {
                            color: Color::srgb(0.6, 0.2, 1.0),
                            intensity: 60_000.0,
                            range: 16.0,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 3.5, 0.0),
                    ));
                });
        }
    }
    let visible = if sim.core_drill().is_some() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut visibility in &mut spires {
        *visibility = visible;
    }
}

/// Le Grand foreur : l'arbre tourne quand il a du combustible, l'anneau s'illumine avec
/// l'avancement, et la Gemme Noire apparaît à l'extraction.
#[allow(clippy::type_complexity)]
fn animate_core_drill(
    sim: Res<Sim>,
    time: Res<Time>,
    mut shafts: Query<(&CoreShaft, &mut Transform), (Without<CoreRing>, Without<BlackGemView>)>,
    mut rings: Query<&mut Transform, (With<CoreRing>, Without<CoreShaft>, Without<BlackGemView>)>,
    mut gems: Query<
        (&mut Transform, &mut Visibility),
        (With<BlackGemView>, Without<CoreShaft>, Without<CoreRing>),
    >,
) {
    for (shaft, mut transform) in &mut shafts {
        if let Some(BuildingState::CoreDrill { fuel, .. }) =
            sim.buildings.get(shaft.0).map(|b| &b.state)
            && *fuel > 0
        {
            transform.rotate_y(time.delta_secs() * 4.0);
        }
    }
    let progress = sim.drilling_percent() as f32 / 100.0;
    for mut transform in &mut rings {
        transform.scale = Vec3::splat(0.6 + 0.8 * progress);
        transform.translation.y = 0.45 + 2.8 * progress;
    }
    let extracted = sim.extraction_until.is_some();
    for (mut transform, mut visibility) in &mut gems {
        *visibility = if extracted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.rotate_y(time.delta_secs() * 0.8);
        transform.translation.y = 5.5 + 0.3 * (time.elapsed_secs() * 1.5).sin();
    }
}
