//! Construction de la planète en 3D à partir de la carte de la simulation.

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use gem_sim::{GemColor, Map, Pos, Simulation, Terrain, Tile, UNITS_PER_TILE, rng::Rng};

use crate::{
    camera::RtsCamera,
    curvature::{CurvedMaterial, curved},
    palette::{self, gem_rgb},
    sim::{NewPlanet, Sim},
};

/// Taille d'une case de la grille, en unités du monde.
pub const TILE_SIZE: f32 = 1.0;
/// Conversion des hauteurs de la simulation (centimètres) en unités du monde.
const HEIGHT_SCALE: f32 = 1.0 / 110.0;

/// Tout ce qui appartient à la planète courante (supprimé quand on en génère une nouvelle).
#[derive(Component)]
pub struct PlanetEntity;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_planet)
            .add_systems(Update, respawn_planet.run_if(on_message::<NewPlanet>));
    }
}

pub fn half_extent(map: &Map) -> f32 {
    map.size as f32 * TILE_SIZE / 2.0
}

fn cm_to_world(cm: i32) -> f32 {
    cm as f32 * HEIGHT_SCALE
}

pub fn water_height(map: &Map) -> f32 {
    cm_to_world(map.water_level_cm) - 0.05
}

/// Position du monde d'un sommet de la grille.
fn vertex_position(map: &Map, x: u32, y: u32) -> Vec3 {
    let half = half_extent(map);
    Vec3::new(
        x as f32 * TILE_SIZE - half,
        cm_to_world(map.vertex_height(x, y)),
        y as f32 * TILE_SIZE - half,
    )
}

/// Centre (au sol) d'une case.
fn tile_center(map: &Map, x: u32, y: u32) -> Vec3 {
    let half = half_extent(map);
    Vec3::new(
        (x as f32 + 0.5) * TILE_SIZE - half,
        cm_to_world(map.tile_height(x, y)),
        (y as f32 + 0.5) * TILE_SIZE - half,
    )
}

/// Position (au sol) d'une position fine de la simulation.
pub fn pos_to_world(map: &Map, pos: Pos) -> Vec3 {
    let half = half_extent(map);
    let xz = Vec2::new(
        pos.x as f32 / UNITS_PER_TILE as f32 * TILE_SIZE - half,
        pos.y as f32 / UNITS_PER_TILE as f32 * TILE_SIZE - half,
    );
    Vec3::new(xz.x, ground_height(map, xz), xz.y)
}

/// Centre (au sol) d'une case de la simulation.
pub fn tile_to_world(map: &Map, tile: Tile) -> Vec3 {
    pos_to_world(map, tile.center())
}

/// Case de la simulation sous un point du monde.
pub fn world_to_tile(map: &Map, point: Vec3) -> Tile {
    let half = half_extent(map);
    Tile::new(
        ((point.x + half) / TILE_SIZE).floor() as i32,
        ((point.z + half) / TILE_SIZE).floor() as i32,
    )
}

/// Centre du QG, au sol (sur le plan horizontal x, z).
pub fn hq_center(sim: &Simulation) -> Vec2 {
    pos_to_world(&sim.map, sim.hq().center()).xz()
}

/// Hauteur du sol sous un point du monde (interpolée entre les sommets).
pub fn ground_height(map: &Map, point: Vec2) -> f32 {
    let half = half_extent(map);
    let gx = ((point.x + half) / TILE_SIZE).clamp(0.0, map.size as f32 - 0.001);
    let gy = ((point.y + half) / TILE_SIZE).clamp(0.0, map.size as f32 - 0.001);
    let (x, y) = (gx as u32, gy as u32);
    let (fx, fy) = (gx.fract(), gy.fract());
    let h = |dx, dy| cm_to_world(map.vertex_height(x + dx, y + dy));
    let top = h(0, 0) + (h(1, 0) - h(0, 0)) * fx;
    let bottom = h(0, 1) + (h(1, 1) - h(0, 1)) * fx;
    top + (bottom - top) * fy
}

pub fn respawn_planet(
    mut commands: Commands,
    planet: Query<Entity, With<PlanetEntity>>,
    sim: Res<Sim>,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<CurvedMaterial>>,
    mut camera: Single<&mut RtsCamera>,
) {
    for entity in &planet {
        commands.entity(entity).despawn();
    }
    camera.look_at(hq_center(&sim));
    spawn_planet(commands, sim, meshes, materials);
}

fn spawn_planet(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CurvedMaterial>>,
) {
    let map = &sim.map;

    // Sol : couleurs portées par les sommets, un seul matériau.
    commands.spawn((
        PlanetEntity,
        Mesh3d(meshes.add(terrain_mesh(map))),
        MeshMaterial3d(materials.add(curved(StandardMaterial {
            perceptual_roughness: 0.92,
            ..default()
        }))),
    ));

    // Eau : un plan subdivisé (sinon la courbure ne pourrait pas le plier).
    let extent = map.size as f32 * TILE_SIZE;
    commands.spawn((
        PlanetEntity,
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(extent, extent)
                    .subdivisions(96),
            ),
        ),
        MeshMaterial3d(materials.add(curved(StandardMaterial {
            base_color: Color::srgba(0.05, 0.32, 0.42, 0.82),
            perceptual_roughness: 0.06,
            reflectance: 0.6,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }))),
        Transform::from_xyz(0.0, water_height(map), 0.0),
        NotShadowCaster,
    ));

    spawn_props(&mut commands, map, &mut meshes, &mut materials);
    spawn_headquarters(
        &mut commands,
        map,
        hq_center(&sim),
        &mut meshes,
        &mut materials,
    );
}

/// Couleur d'une case : teinte du terrain, plus claire en altitude, avec une légère
/// variation pour éviter l'aspect « damier ».
fn tile_color(map: &Map, x: u32, y: u32) -> [f32; 4] {
    let altitude = ((map.tile_height(x, y) - map.water_level_cm) as f32 / 700.0).clamp(0.0, 1.0);
    let base: [f32; 3] = match map.tile(x, y) {
        Terrain::Plain => [
            0.42 + 0.2 * altitude,
            0.2 + 0.17 * altitude,
            0.13 + 0.1 * altitude,
        ],
        Terrain::Rock => [0.24, 0.21, 0.22],
        Terrain::Water => [0.18, 0.2, 0.2],
        Terrain::CrystalVein => {
            // Le sol d'un filon prend une teinte sombre de la couleur de son cristal.
            let tint = gem_rgb(map.vein_color(x, y)) * 0.35 + Vec3::splat(0.06);
            [tint.x, tint.y, tint.z]
        }
        Terrain::Ore => [0.36, 0.4, 0.46],
    };
    let jitter =
        (gem_sim::rng::hash2(map.seed ^ 0xC010, x as i32, y as i32) % 1000) as f32 / 1000.0;
    let shade = 0.92 + 0.16 * jitter;
    [base[0] * shade, base[1] * shade, base[2] * shade, 1.0]
}

/// Maillage du sol : 4 sommets par case (couleurs nettes), normales lissées sur le relief.
fn terrain_mesh(map: &Map) -> Mesh {
    let tiles = (map.size * map.size) as usize;
    let mut positions = Vec::with_capacity(tiles * 4);
    let mut normals = Vec::with_capacity(tiles * 4);
    let mut colors = Vec::with_capacity(tiles * 4);
    let mut indices = Vec::with_capacity(tiles * 6);

    let smooth_normal = |x: u32, y: u32| {
        let h = |x: u32, y: u32| cm_to_world(map.vertex_height(x.min(map.size), y.min(map.size)));
        let dx = h(x + 1, y) - h(x.saturating_sub(1), y);
        let dz = h(x, y + 1) - h(x, y.saturating_sub(1));
        Vec3::new(-dx, 2.0 * TILE_SIZE, -dz).normalize().to_array()
    };

    for y in 0..map.size {
        for x in 0..map.size {
            let first = positions.len() as u32;
            let color = tile_color(map, x, y);
            for (cx, cy) in [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                positions.push(vertex_position(map, cx, cy).to_array());
                normals.push(smooth_normal(cx, cy));
                colors.push(color);
            }
            indices.extend_from_slice(&[
                first,
                first + 2,
                first + 1,
                first + 1,
                first + 2,
                first + 3,
            ]);
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Cristal : prisme hexagonal à pointe, facettes nettes.
pub fn crystal_mesh() -> Mesh {
    const SIDES: usize = 6;
    let ring = |y: f32, r: f32| -> Vec<Vec3> {
        (0..SIDES)
            .map(|i| {
                let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
                Vec3::new(a.cos() * r, y, a.sin() * r)
            })
            .collect()
    };
    let bottom = ring(0.0, 0.16);
    let top = ring(0.75, 0.2);
    let tip = Vec3::new(0.0, 1.05, 0.0);

    let mut triangles: Vec<[Vec3; 3]> = Vec::new();
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        triangles.push([bottom[i], top[i], bottom[j]]);
        triangles.push([bottom[j], top[i], top[j]]);
        triangles.push([top[i], tip, top[j]]);
    }
    flat_mesh(&triangles)
}

/// Maillage à facettes plates (une normale par triangle) : l'aspect « taillé » des gemmes.
pub fn flat_mesh(triangles: &[[Vec3; 3]]) -> Mesh {
    let mut positions = Vec::with_capacity(triangles.len() * 3);
    let mut normals = Vec::with_capacity(triangles.len() * 3);
    for [a, b, c] in triangles {
        let normal = (*c - *a).cross(*b - *a).normalize();
        for p in [a, b, c] {
            positions.push(p.to_array());
            normals.push(normal.to_array());
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

/// Décor : cristaux sur les filons, blocs de minerai, rochers.
fn spawn_props(
    commands: &mut Commands,
    map: &Map,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<CurvedMaterial>,
) {
    let crystal = meshes.add(crystal_mesh());
    let chunk = meshes.add(Cuboid::new(0.4, 0.3, 0.35));
    let boulder = meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap());

    let crystal_materials = GemColor::ALL.map(|color| {
        let tint = gem_rgb(color);
        materials.add(curved(StandardMaterial {
            base_color: Color::srgb(tint.x, tint.y, tint.z),
            emissive: palette::gem_glow(color, 4.5),
            perceptual_roughness: 0.08,
            metallic: 0.1,
            reflectance: 0.9,
            ..default()
        }))
    });
    let ore_material = materials.add(curved(StandardMaterial {
        base_color: Color::srgb(0.55, 0.6, 0.66),
        perceptual_roughness: 0.35,
        metallic: 0.85,
        ..default()
    }));
    let rock_material = materials.add(curved(StandardMaterial {
        base_color: Color::srgb(0.2, 0.18, 0.19),
        perceptual_roughness: 0.95,
        ..default()
    }));

    // Décor purement visuel : son placement n'influence pas la simulation.
    let mut rng = Rng::new(map.seed ^ 0xDEC0);
    for (x, y, terrain) in map.tiles() {
        let center = tile_center(map, x, y);
        let jitter = |rng: &mut Rng| (rng.below(1000) as f32 / 1000.0 - 0.5) * 0.6 * TILE_SIZE;
        match terrain {
            Terrain::CrystalVein if rng.below(100) < 45 => {
                for _ in 0..rng.range(1, 3) {
                    let scale = 0.5 + rng.below(100) as f32 / 100.0;
                    let tilt = Quat::from_euler(
                        EulerRot::YXZ,
                        rng.below(628) as f32 / 100.0,
                        (rng.below(50) as f32 - 25.0).to_radians(),
                        (rng.below(50) as f32 - 25.0).to_radians(),
                    );
                    commands.spawn((
                        PlanetEntity,
                        Mesh3d(crystal.clone()),
                        MeshMaterial3d(crystal_materials[map.vein_color(x, y).index()].clone()),
                        Transform::from_translation(
                            center + Vec3::new(jitter(&mut rng), -0.1, jitter(&mut rng)),
                        )
                        .with_rotation(tilt)
                        .with_scale(Vec3::splat(scale)),
                    ));
                }
            }
            Terrain::Ore if rng.below(100) < 30 => {
                commands.spawn((
                    PlanetEntity,
                    Mesh3d(chunk.clone()),
                    MeshMaterial3d(ore_material.clone()),
                    Transform::from_translation(
                        center + Vec3::new(jitter(&mut rng), 0.05, jitter(&mut rng)),
                    )
                    .with_rotation(Quat::from_rotation_y(rng.below(628) as f32 / 100.0)),
                ));
            }
            Terrain::Rock if rng.below(100) < 6 => {
                let scale = Vec3::new(1.0, 0.6, 0.8) * (0.6 + rng.below(100) as f32 / 80.0);
                commands.spawn((
                    PlanetEntity,
                    Mesh3d(boulder.clone()),
                    MeshMaterial3d(rock_material.clone()),
                    Transform::from_translation(center)
                        .with_rotation(Quat::from_rotation_y(rng.below(628) as f32 / 100.0))
                        .with_scale(scale),
                ));
            }
            _ => {}
        }
    }
}

/// Le QG : la capsule d'atterrissage, au centre du plateau.
fn spawn_headquarters(
    commands: &mut Commands,
    map: &Map,
    center: Vec2,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<CurvedMaterial>,
) {
    let origin = Vec3::new(center.x, cm_to_world(map.landing_height_cm), center.y);
    let hull = materials.add(curved(StandardMaterial {
        base_color: Color::srgb(0.82, 0.84, 0.88),
        metallic: 0.9,
        perceptual_roughness: 0.28,
        ..default()
    }));
    let pad = materials.add(curved(StandardMaterial {
        base_color: Color::srgb(0.32, 0.33, 0.36),
        perceptual_roughness: 0.7,
        metallic: 0.3,
        ..default()
    }));
    let glow = materials.add(curved(StandardMaterial {
        base_color: Color::srgb(0.2, 0.9, 1.0),
        emissive: LinearRgba::rgb(0.6, 5.0, 7.0),
        ..default()
    }));

    let parts: [(Handle<Mesh>, &Handle<CurvedMaterial>, Transform); 5] = [
        (
            meshes.add(Cylinder::new(2.3, 0.25).mesh().resolution(6)),
            &pad,
            Transform::from_translation(origin + Vec3::Y * 0.12),
        ),
        (
            meshes.add(Cylinder::new(1.6, 2.2).mesh().resolution(12)),
            &hull,
            Transform::from_translation(origin + Vec3::Y * 1.35),
        ),
        (
            meshes.add(Sphere::new(1.6).mesh().uv(24, 12)),
            &hull,
            Transform::from_translation(origin + Vec3::Y * 2.45)
                .with_scale(Vec3::new(1.0, 0.7, 1.0)),
        ),
        (
            meshes.add(Torus::new(1.7, 1.85)),
            &glow,
            Transform::from_translation(origin + Vec3::Y * 1.0),
        ),
        (
            meshes.add(Cylinder::new(0.08, 2.0)),
            &glow,
            Transform::from_translation(origin + Vec3::Y * 4.1),
        ),
    ];
    for (mesh, material, transform) in parts {
        commands.spawn((
            PlanetEntity,
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            transform,
        ));
    }

    // Pieds d'atterrissage.
    let leg = meshes.add(Cuboid::new(0.25, 1.6, 0.25));
    for i in 0..4 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
        let dir = Vec3::new(angle.cos(), 0.0, angle.sin());
        commands.spawn((
            PlanetEntity,
            Mesh3d(leg.clone()),
            MeshMaterial3d(hull.clone()),
            Transform::from_translation(origin + Vec3::Y * 0.8 + dir * 1.75)
                .with_rotation(Quat::from_axis_angle(dir.cross(Vec3::Y), -0.35)),
        ));
    }

    // Balise lumineuse du QG.
    commands.spawn((
        PlanetEntity,
        PointLight {
            color: Color::srgb(0.3, 0.9, 1.0),
            intensity: 60_000.0,
            range: 25.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(origin + Vec3::Y * 5.5),
    ));
}
