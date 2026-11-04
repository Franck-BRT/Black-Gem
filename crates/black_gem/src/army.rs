//! Armée, façon RTS classique : les unités (Sentinelles, Blindés et le héros, le
//! Forgeron-Ingénieur), leur sélection, leurs ordres, les pouvoirs du héros et la formation
//! à la caserne. À la souris et au clavier comme au doigt.
//!
//! - clic gauche sur une unité : la sélectionner (Maj : l'ajouter) ; glisser : sélection
//!   au cadre ; clic dans le vide : tout désélectionner ;
//! - clic droit : y aller en combattant (Ctrl + clic droit : y aller sans s'arrêter) ;
//! - au doigt : toucher une unité la sélectionne, toucher le sol y envoie la sélection ;
//! - Tab : le héros, puis toute l'armée ; J / K : pouvoirs ; Entrée : former une unité.

use std::collections::HashMap;

use bevy::{
    light::NotShadowCaster,
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_sim::{
    Ability, BuildingKind, BuildingState, Command, Order, Pos, TICKS_PER_SECOND, Tile, UnitKind,
    building::BARRACKS_QUEUE,
};

use crate::{
    build::{BlocksWorldInput, BuildState, UiBlockers, pick_tile, point_over_ui, pointer_over_ui},
    camera::RtsCamera,
    curvature::{CurrentCurvature, CurvedMaterial, bend, curved},
    i18n::{Language, Tr},
    mission::{MissionMenu, playing},
    particles::{Burst, Style},
    sim::{NewPlanet, Sim},
    view::NestView,
    world::{self, PlanetEntity},
};

/// Unités sélectionnées (identifiants de la simulation).
#[derive(Resource, Default)]
pub struct Selection {
    pub units: Vec<u32>,
}

/// Cadre de sélection en cours (coordonnées de la fenêtre).
#[derive(Resource, Default)]
struct DragBox {
    start: Option<Vec2>,
    current: Vec2,
    active: bool,
}

/// Points visés par les derniers ordres : un cercle qui se resserre.
#[derive(Resource, Default)]
struct Markers(Vec<(Vec3, f32, bool)>);

/// Pouvoirs lancés : une onde qui s'élargit (centre, instant, pouvoir).
#[derive(Resource, Default)]
struct Waves(Vec<(Vec3, f32, Ability)>);

const WAVE_SECONDS: f32 = 0.9;

#[derive(Resource, Default)]
struct UnitViews(HashMap<u32, Entity>);

#[derive(Component)]
struct UnitView {
    yaw: f32,
}

#[derive(Component)]
struct SelectBox;

#[derive(Component)]
struct ArmyPanel;

#[derive(Component)]
struct ArmyText;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PanelButton {
    Ability(Ability),
    Train,
    Hero,
    Army,
}

#[derive(Resource)]
struct ArmyKit {
    body: Handle<Mesh>,
    visor: Handle<Mesh>,
    hull: Handle<Mesh>,
    turret: Handle<Mesh>,
    barrel: Handle<Mesh>,
    hero_body: Handle<Mesh>,
    halo: Handle<Mesh>,
    hero_gem: Handle<Mesh>,
    armor: Handle<CurvedMaterial>,
    plate: Handle<CurvedMaterial>,
    gold: Handle<CurvedMaterial>,
    cyan: Handle<CurvedMaterial>,
    purple: Handle<CurvedMaterial>,
}

pub struct ArmyPlugin;

impl Plugin for ArmyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>()
            .init_resource::<DragBox>()
            .init_resource::<Markers>()
            .init_resource::<UnitViews>()
            .init_resource::<Waves>()
            .add_systems(Startup, (create_kit, spawn_panel))
            .add_systems(
                Update,
                (
                    reset.run_if(on_message::<NewPlanet>),
                    (
                        hotkeys.run_if(not(crate::editor::typing)),
                        mouse_input,
                        touch_input,
                    )
                        .run_if(playing)
                        .before(crate::build::mouse_build),
                    sync_units,
                    update_nests,
                    ability_effects,
                    draw_selection,
                    update_panel,
                )
                    .chain()
                    .after(world::respawn_planet),
            );
    }
}

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
    commands.insert_resource(ArmyKit {
        body: meshes.add(Capsule3d::new(0.17, 0.36)),
        visor: meshes.add(Cuboid::new(0.12, 0.07, 0.24)),
        hull: meshes.add(Cuboid::new(0.82, 0.3, 0.6)),
        turret: meshes.add(Cuboid::new(0.4, 0.2, 0.34)),
        barrel: meshes.add(Cuboid::new(0.5, 0.08, 0.08)),
        hero_body: meshes.add(Capsule3d::new(0.22, 0.5)),
        halo: meshes.add(Torus::new(0.3, 0.36)),
        hero_gem: meshes.add(Sphere::new(0.13).mesh().ico(1).unwrap()),
        armor: material(StandardMaterial {
            base_color: Color::srgb(0.32, 0.36, 0.46),
            metallic: 0.8,
            perceptual_roughness: 0.35,
            ..default()
        }),
        plate: material(StandardMaterial {
            base_color: Color::srgb(0.85, 0.87, 0.9),
            metallic: 0.85,
            perceptual_roughness: 0.3,
            ..default()
        }),
        gold: material(StandardMaterial {
            base_color: Color::srgb(0.95, 0.72, 0.3),
            metallic: 0.95,
            perceptual_roughness: 0.25,
            ..default()
        }),
        cyan: material(glow(
            Color::srgb(0.2, 0.9, 1.0),
            LinearRgba::rgb(0.4, 3.0, 4.0),
        )),
        purple: material(glow(
            Color::srgb(0.6, 0.2, 1.0),
            LinearRgba::rgb(2.5, 0.6, 5.0),
        )),
    });
}

fn reset(
    mut views: ResMut<UnitViews>,
    mut selection: ResMut<Selection>,
    mut markers: ResMut<Markers>,
) {
    // Les entités disparaissent avec la planète (`PlanetEntity`).
    views.0.clear();
    selection.units.clear();
    markers.0.clear();
}

/// Position affichée d'une unité (interpolée entre deux ticks).
fn unit_world(sim: &Sim, prev: Pos, pos: Pos, alpha: f32) -> Vec3 {
    let from = world::pos_to_world(&sim.map, prev);
    let to = world::pos_to_world(&sim.map, pos);
    from.lerp(to, alpha)
}

#[allow(clippy::too_many_arguments)]
fn sync_units(
    mut commands: Commands,
    sim: Res<Sim>,
    kit: Res<ArmyKit>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    mut views: ResMut<UnitViews>,
    mut selection: ResMut<Selection>,
    mut roots: Query<(&mut Transform, &mut UnitView)>,
    mut bursts: MessageWriter<Burst>,
) {
    views.0.retain(|id, entity| {
        let alive = sim.units.iter().any(|u| u.id == *id);
        if !alive {
            if let Ok((transform, _)) = roots.get(*entity) {
                bursts.write(Burst::new(transform.translation, Style::Spark(None), 24));
            }
            commands.entity(*entity).despawn();
        }
        alive
    });
    selection
        .units
        .retain(|id| sim.units.iter().any(|u| u.id == *id));

    let alpha = fixed.overstep_fraction();
    let t = time.elapsed_secs();
    for unit in &sim.units {
        let at = unit_world(&sim, unit.prev_pos, unit.pos, alpha);
        let moving = unit.pos != unit.prev_pos;
        match views.0.get(&unit.id) {
            Some(&entity) => {
                if let Ok((mut transform, mut view)) = roots.get_mut(entity) {
                    if moving {
                        let d = unit.pos;
                        let p = unit.prev_pos;
                        let target = (-((d.y - p.y) as f32)).atan2((d.x - p.x) as f32);
                        let diff = (target - view.yaw + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                        view.yaw += diff * 0.25;
                    }
                    let bob = if moving {
                        0.03 * (t * 14.0 + unit.id as f32).sin().abs()
                    } else {
                        0.0
                    };
                    transform.translation = at + Vec3::Y * bob;
                    transform.rotation = Quat::from_rotation_y(view.yaw);
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        PlanetEntity,
                        UnitView { yaw: 0.0 },
                        Transform::from_translation(at).with_scale(Vec3::splat(1.2)),
                        Visibility::default(),
                    ))
                    .with_children(|parent| match unit.kind {
                        UnitKind::Sentinel => {
                            parent.spawn((
                                Mesh3d(kit.body.clone()),
                                MeshMaterial3d(kit.armor.clone()),
                                Transform::from_xyz(0.0, 0.36, 0.0),
                            ));
                            parent.spawn((
                                Mesh3d(kit.visor.clone()),
                                MeshMaterial3d(kit.cyan.clone()),
                                Transform::from_xyz(0.13, 0.56, 0.0),
                                NotShadowCaster,
                            ));
                            parent.spawn((
                                Mesh3d(kit.barrel.clone()),
                                MeshMaterial3d(kit.plate.clone()),
                                Transform::from_xyz(0.18, 0.38, 0.15)
                                    .with_scale(Vec3::new(0.8, 1.0, 1.0)),
                            ));
                        }
                        UnitKind::Tank => {
                            parent.spawn((
                                Mesh3d(kit.hull.clone()),
                                MeshMaterial3d(kit.armor.clone()),
                                Transform::from_xyz(0.0, 0.2, 0.0),
                            ));
                            parent.spawn((
                                Mesh3d(kit.turret.clone()),
                                MeshMaterial3d(kit.plate.clone()),
                                Transform::from_xyz(-0.05, 0.45, 0.0),
                            ));
                            parent.spawn((
                                Mesh3d(kit.barrel.clone()),
                                MeshMaterial3d(kit.plate.clone()),
                                Transform::from_xyz(0.38, 0.47, 0.0),
                            ));
                            parent.spawn((
                                Mesh3d(kit.visor.clone()),
                                MeshMaterial3d(kit.cyan.clone()),
                                Transform::from_xyz(0.42, 0.25, 0.0)
                                    .with_scale(Vec3::new(0.5, 0.6, 2.2)),
                                NotShadowCaster,
                            ));
                        }
                        UnitKind::Hero => {
                            parent.spawn((
                                Mesh3d(kit.hero_body.clone()),
                                MeshMaterial3d(kit.gold.clone()),
                                Transform::from_xyz(0.0, 0.5, 0.0),
                            ));
                            parent.spawn((
                                Mesh3d(kit.visor.clone()),
                                MeshMaterial3d(kit.purple.clone()),
                                Transform::from_xyz(0.17, 0.78, 0.0),
                                NotShadowCaster,
                            ));
                            parent.spawn((
                                Mesh3d(kit.halo.clone()),
                                MeshMaterial3d(kit.purple.clone()),
                                Transform::from_xyz(0.0, 1.22, 0.0),
                                NotShadowCaster,
                            ));
                            parent.spawn((
                                Mesh3d(kit.hero_gem.clone()),
                                MeshMaterial3d(kit.cyan.clone()),
                                Transform::from_xyz(-0.05, 1.22, 0.0),
                                NotShadowCaster,
                            ));
                        }
                    })
                    .id();
                views.0.insert(unit.id, entity);
            }
        }
    }
}

/// Onde de gel (anneau cyan) et pluie de cristaux (anneau violet et éclats).
fn ability_effects(
    mut gizmos: Gizmos,
    sim: Res<Sim>,
    events: Res<crate::sim::SimEvents>,
    curvature: Res<CurrentCurvature>,
    time: Res<Time<Real>>,
    mut waves: ResMut<Waves>,
    mut bursts: MessageWriter<Burst>,
) {
    let now = time.elapsed_secs();
    for event in events.iter() {
        if let gem_sim::SimEvent::AbilityUsed { ability, pos } = *event {
            let at = world::pos_to_world(&sim.map, pos) + Vec3::Y * 0.3;
            waves.0.push((at, now, ability));
            let radius = ability.radius() as f32 / gem_sim::UNITS_PER_TILE as f32;
            let (color, count) = match ability {
                Ability::FrostWave => (gem_sim::GemColor::Cyan, 10),
                Ability::CrystalRain => (gem_sim::GemColor::Purple, 16),
            };
            for k in 0..count {
                let angle = k as f32 / count as f32 * std::f32::consts::TAU;
                let r = radius * if k % 2 == 0 { 0.9 } else { 0.5 };
                let p = at + Vec3::new(angle.cos() * r, 0.2, angle.sin() * r);
                bursts.write(Burst::new(p, Style::Spark(Some(color)), 6));
            }
        }
    }
    waves.0.retain(|(_, born, _)| now - born < WAVE_SECONDS);
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for &(at, born, ability) in &waves.0 {
        let k = (now - born) / WAVE_SECONDS;
        let radius = ability.radius() as f32 / gem_sim::UNITS_PER_TILE as f32;
        let color = match ability {
            Ability::FrostWave => LinearRgba::rgb(0.5, 3.5, 5.0),
            Ability::CrystalRain => LinearRgba::rgb(3.5, 0.8, 6.0),
        } * (1.0 - k);
        for ring in [1.0, 0.8] {
            gizmos.circle(
                Isometry3d::new(bend(at, curvature.params), flat),
                radius * (0.2 + 0.8 * k) * ring,
                color,
            );
        }
    }
}

/// Nids rasés : ils s'effondrent dans une gerbe d'éclats.
fn update_nests(
    sim: Res<Sim>,
    mut nests: Query<(&NestView, &Transform, &mut Visibility)>,
    mut bursts: MessageWriter<Burst>,
) {
    for (nest, transform, mut visibility) in &mut nests {
        let destroyed = sim.nest_hp.get(nest.0).is_some_and(|&hp| hp <= 0);
        if destroyed && *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
            bursts.write(Burst::new(transform.translation + Vec3::Y, Style::Boss, 70));
        }
    }
}

/// Position à l'écran d'une unité (pour la sélection au pointeur).
fn unit_screen(
    sim: &Sim,
    camera: (&Camera, &GlobalTransform),
    curvature: Vec4,
    unit: &gem_sim::Unit,
) -> Option<Vec2> {
    let at = world::pos_to_world(&sim.map, unit.pos) + Vec3::Y * 0.5;
    camera
        .0
        .world_to_viewport(camera.1, bend(at, curvature))
        .ok()
}

/// L'unité la plus proche d'un point de l'écran (à moins de `radius` pixels).
fn unit_under(
    sim: &Sim,
    camera: (&Camera, &GlobalTransform),
    curvature: Vec4,
    point: Vec2,
    radius: f32,
) -> Option<u32> {
    sim.units
        .iter()
        .filter_map(|u| {
            let screen = unit_screen(sim, camera, curvature, u)?;
            let d = screen.distance(point);
            (d <= radius).then_some((d, u.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// Envoie la sélection vers une case ; renvoie vrai si un ordre est parti.
fn order_to(
    sim: &mut Sim,
    selection: &Selection,
    markers: &mut Markers,
    tile: Tile,
    attack: bool,
    now: f32,
) -> bool {
    if selection.units.is_empty() {
        return false;
    }
    let target = tile.center();
    let order = if attack {
        Order::AttackMove(target)
    } else {
        Order::Move(target)
    };
    sim.queue(Command::Order {
        units: selection.units.clone(),
        order,
    });
    markers
        .0
        .push((world::tile_to_world(&sim.map, tile), now, attack));
    true
}

/// Former une unité à la caserne la moins chargée. Renvoie le refus éventuel.
pub fn train(sim: &mut Sim) -> Option<Tr> {
    let barracks = sim
        .buildings
        .iter()
        .filter_map(|b| match b.state {
            BuildingState::Barracks { queue, .. } => Some((queue, b.tile)),
            _ => None,
        })
        .min_by_key(|(queue, tile)| (*queue, tile.x, tile.y));
    let Some((queue, tile)) = barracks else {
        return Some(Tr::NeedBarracks);
    };
    let stats = sim.trained_kind().stats();
    let (used, cap) = sim.supply();
    if used + stats.supply > cap {
        return Some(Tr::SupplyFull);
    }
    if queue >= BARRACKS_QUEUE {
        return Some(Tr::QueueFull);
    }
    if sim.stock < stats.cost {
        return Some(Tr::Error(gem_sim::BuildError::NotEnoughCrystals));
    }
    sim.queue(Command::Train { tile });
    None
}

/// Lancer un pouvoir du héros. Renvoie le refus éventuel.
pub fn cast(sim: &mut Sim, ability: Ability) -> Option<Tr> {
    let Some(hero) = sim.hero() else {
        return Some(Tr::HeroDown);
    };
    if hero.level < ability.min_level() {
        return Some(Tr::AbilityLocked);
    }
    if sim.tick < sim.ability_ready[ability.index()] {
        return Some(Tr::AbilityCooldown);
    }
    sim.queue(Command::Ability(ability));
    None
}

/// Tab : le héros, puis toute l'armée.
fn select_cycle(sim: &Sim, selection: &mut Selection) {
    let hero = sim.hero().map(|h| h.id);
    if hero.is_some() && selection.units != hero.into_iter().collect::<Vec<_>>() {
        selection.units = hero.into_iter().collect();
    } else {
        selection.units = sim.units.iter().map(|u| u.id).collect();
    }
}

fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut build: ResMut<BuildState>,
    mut camera: Single<&mut RtsCamera>,
) {
    let now = time.elapsed_secs();
    let mut refusal = None;
    if keys.just_pressed(KeyCode::Tab) {
        select_cycle(&sim, &mut selection);
        // Deux fois Tab sur le héros : la caméra le suit.
        if let Some(hero) = sim.hero()
            && selection.units == [hero.id]
        {
            camera.look_at(world::pos_to_world(&sim.map, hero.pos).xz());
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        selection.units.clear();
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        refusal = cast(&mut sim, Ability::FrostWave);
    }
    if keys.just_pressed(KeyCode::KeyK) {
        refusal = cast(&mut sim, Ability::CrystalRain);
    }
    if keys.just_pressed(KeyCode::Enter) {
        refusal = train(&mut sim);
    }
    if let Some(tr) = refusal {
        build.error = Some((tr, now + 2.5));
    }
}

#[allow(clippy::too_many_arguments)]
fn mouse_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    curvature: Res<CurrentCurvature>,
    hovered: Query<&Hovered, With<BlocksWorldInput>>,
    build: Res<BuildState>,
    time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut drag: ResMut<DragBox>,
    mut markers: ResMut<Markers>,
    mut right_press: Local<Option<Vec2>>,
) {
    let cursor = window.cursor_position();
    let camera = *camera;
    if build.tool.is_some() {
        drag.start = None;
        drag.active = false;
        *right_press = None;
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if buttons.just_pressed(MouseButton::Left) && !pointer_over_ui(&hovered) {
        drag.start = cursor;
        drag.active = false;
    }
    if let (Some(start), Some(now)) = (drag.start, cursor) {
        drag.current = now;
        if start.distance(now) > 8.0 {
            drag.active = true;
        }
    }
    if buttons.just_released(MouseButton::Left)
        && let Some(start) = drag.start.take()
    {
        let end = cursor.unwrap_or(drag.current);
        if !shift {
            selection.units.clear();
        }
        if drag.active {
            let (lo, hi) = (start.min(end), start.max(end));
            for unit in &sim.units {
                if let Some(p) = unit_screen(&sim, camera, curvature.params, unit)
                    && p.x >= lo.x
                    && p.y >= lo.y
                    && p.x <= hi.x
                    && p.y <= hi.y
                    && !selection.units.contains(&unit.id)
                {
                    selection.units.push(unit.id);
                }
            }
        } else if let Some(id) = unit_under(&sim, camera, curvature.params, end, 24.0) {
            match selection.units.iter().position(|&u| u == id) {
                Some(i) if shift => {
                    selection.units.remove(i);
                }
                Some(_) => {}
                None => selection.units.push(id),
            }
        }
        drag.active = false;
    }

    // Clic droit bref : un ordre (glisser sert à orienter la caméra).
    if buttons.just_pressed(MouseButton::Right) {
        *right_press = cursor.filter(|_| !pointer_over_ui(&hovered));
    }
    if buttons.just_released(MouseButton::Right)
        && let (Some(a), Some(b)) = (right_press.take(), cursor)
        && a.distance(b) < 6.0
        && let Some(tile) = pick_tile(b, camera, &sim, curvature.params)
    {
        let attack = !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
        order_to(
            &mut sim,
            &selection,
            &mut markers,
            tile,
            attack,
            time.elapsed_secs(),
        );
    }
}

/// Au doigt : toucher une unité la sélectionne ; toucher le sol y envoie la sélection.
#[allow(clippy::too_many_arguments)]
fn touch_input(
    touches: Res<Touches>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    curvature: Res<CurrentCurvature>,
    blockers: UiBlockers,
    build: Res<BuildState>,
    time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut markers: ResMut<Markers>,
) {
    if build.tool.is_some() || touches.iter().count() > 0 {
        return;
    }
    let scale = window.scale_factor();
    let camera = *camera;
    for touch in touches.iter_just_released() {
        let tap = touch.start_position().distance(touch.position()) < 12.0;
        if !tap
            || point_over_ui(touch.start_position(), scale, &blockers)
            || point_over_ui(touch.position(), scale, &blockers)
        {
            continue;
        }
        if let Some(id) = unit_under(&sim, camera, curvature.params, touch.position(), 34.0) {
            selection.units = vec![id];
        } else if let Some(tile) = pick_tile(touch.position(), camera, &sim, curvature.params) {
            order_to(
                &mut sim,
                &selection,
                &mut markers,
                tile,
                true,
                time.elapsed_secs(),
            );
        }
    }
}

/// Cercles sous les unités choisies, cadre de sélection et points visés.
#[allow(clippy::too_many_arguments)]
fn draw_selection(
    mut gizmos: Gizmos,
    sim: Res<Sim>,
    selection: Res<Selection>,
    drag: Res<DragBox>,
    curvature: Res<CurrentCurvature>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    ui_scale: Res<UiScale>,
    mut markers: ResMut<Markers>,
    mut select_box: Single<&mut Node, With<SelectBox>>,
) {
    let alpha = fixed.overstep_fraction();
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for unit in sim.units.iter().filter(|u| selection.units.contains(&u.id)) {
        let at = unit_world(&sim, unit.prev_pos, unit.pos, alpha) + Vec3::Y * 0.06;
        let (radius, color) = match unit.kind {
            UnitKind::Hero => (0.6, LinearRgba::rgb(3.0, 2.2, 0.6)),
            UnitKind::Tank => (0.6, LinearRgba::rgb(0.4, 3.0, 1.0)),
            UnitKind::Sentinel => (0.4, LinearRgba::rgb(0.4, 3.0, 1.0)),
        };
        gizmos.circle(
            Isometry3d::new(bend(at, curvature.params), flat),
            radius,
            color,
        );
    }
    let now = time.elapsed_secs();
    markers.0.retain(|(_, born, _)| now - born < 0.7);
    for &(at, born, attack) in &markers.0 {
        let k = (now - born) / 0.7;
        let color = if attack {
            LinearRgba::rgb(4.0, 0.8, 0.4)
        } else {
            LinearRgba::rgb(0.4, 3.0, 1.2)
        } * (1.0 - k);
        gizmos.circle(
            Isometry3d::new(bend(at + Vec3::Y * 0.08, curvature.params), flat),
            0.9 - 0.6 * k,
            color,
        );
    }

    let display = match drag.start {
        Some(start) if drag.active => {
            let scale = ui_scale.0.max(0.01);
            let (lo, hi) = (start.min(drag.current), start.max(drag.current));
            select_box.left = Val::Px(lo.x / scale);
            select_box.top = Val::Px(lo.y / scale);
            select_box.width = Val::Px((hi.x - lo.x) / scale);
            select_box.height = Val::Px((hi.y - lo.y) / scale);
            Display::Flex
        }
        _ => Display::None,
    };
    if select_box.display != display {
        select_box.display = display;
    }
}

const PANEL: Color = Color::srgba(0.02, 0.01, 0.05, 0.6);
const BUTTON: Color = Color::srgba(0.12, 0.1, 0.18, 0.85);
const BUTTON_READY: Color = Color::srgba(0.3, 0.16, 0.5, 0.95);

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        SelectBox,
        Node {
            position_type: PositionType::Absolute,
            border: UiRect::all(Val::Px(1.0)),
            display: Display::None,
            ..default()
        },
        BorderColor::all(Color::srgb(0.4, 1.0, 0.6)),
        BackgroundColor(Color::srgba(0.4, 1.0, 0.6, 0.08)),
        Pickable::IGNORE,
    ));
    commands
        .spawn((
            ArmyPanel,
            crate::blocks::HideWhileCoding,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(258.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(8.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                max_width: Val::Px(340.0),
                ..default()
            },
            BackgroundColor(PANEL),
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|panel| {
            panel.spawn((
                ArmyText,
                Text::default(),
                TextFont {
                    font_size: 13.0.into(),
                    ..default()
                },
            ));
            {
                let row = [
                    PanelButton::Hero,
                    PanelButton::Army,
                    PanelButton::Ability(Ability::FrostWave),
                    PanelButton::Ability(Ability::CrystalRain),
                    PanelButton::Train,
                ];
                panel
                    .spawn(Node {
                        column_gap: Val::Px(6.0),
                        row_gap: Val::Px(6.0),
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    })
                    .with_children(|row_node| {
                        for button in row {
                            row_node
                                .spawn((
                                    button,
                                    Node {
                                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                                        border_radius: BorderRadius::all(Val::Px(5.0)),
                                        ..default()
                                    },
                                    BackgroundColor(BUTTON),
                                ))
                                .observe(press)
                                .with_children(|b| {
                                    b.spawn((
                                        Text::default(),
                                        TextFont {
                                            font_size: 13.0.into(),
                                            ..default()
                                        },
                                        Pickable::IGNORE,
                                    ));
                                });
                        }
                    });
            }
        });
}

fn press(
    click: On<PointerClick>,
    buttons: Query<&PanelButton>,
    time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut selection: ResMut<Selection>,
    mut build: ResMut<BuildState>,
    mut camera: Single<&mut RtsCamera>,
) {
    let Ok(button) = buttons.get(click.entity) else {
        return;
    };
    let refusal = match *button {
        PanelButton::Ability(ability) => cast(&mut sim, ability),
        PanelButton::Train => train(&mut sim),
        PanelButton::Hero => {
            match sim.hero() {
                Some(hero) => {
                    selection.units = vec![hero.id];
                    camera.look_at(world::pos_to_world(&sim.map, hero.pos).xz());
                }
                None => selection.units.clear(),
            }
            None
        }
        PanelButton::Army => {
            selection.units = sim.units.iter().map(|u| u.id).collect();
            None
        }
    };
    if let Some(tr) = refusal {
        build.error = Some((tr, time.elapsed_secs() + 2.5));
    }
}

fn seconds_left(sim: &Sim, until: u64) -> u64 {
    until
        .saturating_sub(sim.tick)
        .div_ceil(TICKS_PER_SECOND as u64)
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    sim: Res<Sim>,
    menu: Res<MissionMenu>,
    effective: Res<crate::settings::Effective>,
    selection: Res<Selection>,
    language: Res<Language>,
    mut panel: Single<&mut Node, With<ArmyPanel>>,
    mut info: Single<&mut Text, With<ArmyText>>,
    buttons: Query<(&PanelButton, &Children, &mut BackgroundColor)>,
    mut texts: Query<&mut Text, Without<ArmyText>>,
) {
    let display = if menu.open {
        Display::None
    } else {
        Display::Flex
    };
    if panel.display != display {
        panel.display = display;
    }
    if menu.open {
        return;
    }
    // Au doigt : une seule rangée, en bas à gauche, au-dessus de la barre tactile.
    let touch = effective.touch_ui;
    let (top, bottom, direction, max_width) = if touch {
        (
            Val::Auto,
            Val::Px(150.0),
            FlexDirection::Row,
            Val::Percent(96.0),
        )
    } else {
        (
            Val::Px(258.0),
            Val::Auto,
            FlexDirection::Column,
            Val::Px(340.0),
        )
    };
    if panel.top != top || panel.flex_direction != direction {
        panel.top = top;
        panel.bottom = bottom;
        panel.flex_direction = direction;
        panel.max_width = max_width;
        panel.column_gap = Val::Px(10.0);
        panel.align_items = AlignItems::Center;
    }
    let french = *language == Language::French;
    let t = |fr: &'static str, en: &'static str| if french { fr } else { en };
    let hero_line = match (sim.hero(), &sim.fallen_hero) {
        (Some(hero), _) => format!(
            "{} · {} {} · {} {}/{} · XP {}/{}",
            t("Forgeron-Ingénieur", "Smith-Engineer"),
            t("niv.", "lvl"),
            hero.level,
            t("PV", "HP"),
            hero.hp.max(0),
            hero.max_hp,
            hero.xp % gem_sim::unit::XP_PER_LEVEL,
            gem_sim::unit::XP_PER_LEVEL
        ),
        (None, Some((_, at))) => format!(
            "{} {} s",
            t("Le héros revient dans", "The hero returns in"),
            seconds_left(&sim, *at)
        ),
        (None, None) => String::new(),
    };
    let soldiers = sim
        .units
        .iter()
        .filter(|u| u.kind != UnitKind::Hero)
        .count();
    let queued: u32 = sim
        .buildings
        .iter()
        .map(|b| match b.state {
            BuildingState::Barracks { queue, .. } => queue,
            _ => 0,
        })
        .sum();
    let (used, cap) = sim.supply();
    let (produced, consumed) = sim.power;
    let mut army_line = format!(
        "{} {soldiers}{} · {} {used}/{cap} · {} {produced}/{consumed}",
        t("Soldats", "Soldiers"),
        if queued > 0 {
            format!(" (+{queued})")
        } else {
            String::new()
        },
        t("Ravit.", "Supply"),
        t("Énergie", "Power"),
    );
    if !selection.units.is_empty() {
        army_line.push_str(&format!(
            "\n{} : {} — {}",
            t("Sélection", "Selected"),
            selection.units.len(),
            t(
                "clic droit (ou toucher le sol) pour l'envoyer",
                "right click (or tap the ground) to send it"
            )
        ));
    }
    let content = if touch {
        let hp = sim.hero().map_or("—".to_string(), |h| {
            format!("{} {}", t("niv.", "lvl"), h.level)
        });
        format!(
            "{} {hp}\n{} {used}/{cap} · {} {produced}/{consumed}",
            t("Héros", "Hero"),
            t("Ravit.", "Supply"),
            t("Énergie", "Power")
        )
    } else {
        format!("{hero_line}\n{army_line}")
    };
    if info.0 != content {
        info.0 = content;
    }

    let has_barracks = sim
        .buildings
        .iter()
        .any(|b| b.kind == BuildingKind::Barracks);
    let level = sim.hero().map_or(0, |h| h.level);
    let trained = sim.trained_kind();
    for (button, children, mut background) in buttons {
        let (label, ready) = match *button {
            PanelButton::Ability(ability) => {
                let (name, key) = match ability {
                    Ability::FrostWave => (t("Onde de gel", "Frost wave"), "J"),
                    Ability::CrystalRain => (t("Pluie de cristaux", "Crystal rain"), "K"),
                };
                let ready_at = sim.ability_ready[ability.index()];
                let key = if touch {
                    String::new()
                } else {
                    format!(" [{key}]")
                };
                if level == 0 {
                    (format!("{name}{key}"), false)
                } else if level < ability.min_level() {
                    (
                        format!("{name} · {} {}", t("niv.", "lvl"), ability.min_level()),
                        false,
                    )
                } else if sim.tick < ready_at {
                    (
                        format!("{name} · {} s", seconds_left(&sim, ready_at)),
                        false,
                    )
                } else {
                    (format!("{name}{key}"), true)
                }
            }
            PanelButton::Train => {
                let name = match trained {
                    UnitKind::Tank => t("Blindé", "Tank"),
                    _ => t("Sentinelle", "Sentinel"),
                };
                let key = if touch {
                    String::new()
                } else {
                    format!(" [{}]", t("Entrée", "Enter"))
                };
                (
                    format!(
                        "{} {name} ({}){key}",
                        t("Former", "Train"),
                        trained.stats().cost,
                    ),
                    has_barracks && used + trained.stats().supply <= cap,
                )
            }
            PanelButton::Hero if touch => (t("Héros", "Hero").to_string(), false),
            PanelButton::Hero => (format!("{} [Tab]", t("Héros", "Hero")), false),
            PanelButton::Army => (t("Toute l'armée", "Whole army").to_string(), false),
        };
        let color = if ready { BUTTON_READY } else { BUTTON };
        if background.0 != color {
            background.0 = color;
        }
        if let Some(&child) = children.first()
            && let Ok(mut text) = texts.get_mut(child)
            && text.0 != label
        {
            text.0 = label;
        }
    }
}
