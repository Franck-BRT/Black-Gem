//! Construction : choix de l'outil, case visée, aperçu et envoi des ordres à la simulation.

use bevy::{picking::hover::Hovered, prelude::*};
use gem_sim::{BuildError, BuildingKind, Command, Dir, Gem, Simulation, Tile};

use crate::{
    curvature::{CurrentCurvature, bend},
    i18n::Tr,
    sim::Sim,
    world,
};

/// Outil de construction sélectionné.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Build(BuildingKind),
    Demolish,
    /// Sertir cette gemme de l'inventaire dans une tour.
    Socket(Gem),
    /// Retirer la gemme d'une tour.
    Unsocket,
}

#[derive(Resource, Default)]
pub struct BuildState {
    pub tool: Option<Tool>,
    /// Orientation des prochains bâtiments.
    pub dir: Dir,
    /// Case sous le pointeur (aucune si le pointeur est sur l'interface ou hors de la carte).
    pub cursor: Option<Tile>,
    drag_start: Option<Tile>,
    /// Dernier refus, et l'instant (temps réel) jusqu'auquel l'afficher.
    pub error: Option<(Tr, f32)>,
}

/// Élément d'interface qui « capte » le pointeur : on ne construit pas à travers.
#[derive(Component)]
pub struct BlocksWorldInput;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildState>()
            .add_systems(Startup, configure_gizmos)
            .add_systems(
                Update,
                (
                    hotkeys.run_if(not(crate::editor::typing)),
                    update_cursor,
                    mouse_build,
                    touch_build,
                    draw_preview,
                )
                    .chain(),
            );
    }
}

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.line.width = 3.0;
}

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<BuildState>) {
    let choices = [
        (KeyCode::KeyB, Tool::Build(BuildingKind::Drill)),
        (KeyCode::KeyC, Tool::Build(BuildingKind::Conveyor)),
        (KeyCode::KeyG, Tool::Build(BuildingKind::Cutter)),
        (KeyCode::KeyF, Tool::Build(BuildingKind::Furnace)),
        (KeyCode::KeyT, Tool::Build(BuildingKind::Tower)),
        (KeyCode::KeyX, Tool::Demolish),
        (KeyCode::KeyU, Tool::Unsocket),
    ];
    for (key, tool) in choices {
        if keys.just_pressed(key) {
            state.tool = if state.tool == Some(tool) {
                None
            } else {
                Some(tool)
            };
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        state.dir = state.dir.clockwise();
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.tool = None;
        state.drag_start = None;
    }
}

/// Le pointeur est-il au-dessus d'un élément d'interface ?
fn pointer_over_ui(blockers: &Query<&Hovered, With<BlocksWorldInput>>) -> bool {
    blockers.iter().any(|hovered| hovered.get())
}

/// Case visée par un point de l'écran, en tenant compte du relief et de la courbure.
/// Hauteur approximative d'un bâtiment : un clic sur sa silhouette le désigne,
/// pas la case de sol qui se trouve derrière.
fn building_height(kind: BuildingKind) -> f32 {
    match kind {
        BuildingKind::Hq => 4.0,
        BuildingKind::Tower => 1.7,
        BuildingKind::Furnace => 1.2,
        BuildingKind::Drill | BuildingKind::Cutter => 0.9,
        BuildingKind::Conveyor => 0.15,
    }
}

fn pick_tile(
    screen: Vec2,
    camera: (&Camera, &GlobalTransform),
    sim: &Simulation,
    curvature: Vec4,
) -> Option<Tile> {
    let map = &sim.map;
    let ray = camera.0.viewport_to_world(camera.1, screen).ok()?;
    let direction = *ray.direction;
    let ground_at = |p: Vec3| {
        let flat = world::ground_height(map, p.xz()).max(world::water_height(map));
        bend(Vec3::new(p.x, flat, p.z), curvature).y
    };
    let below_ground = |t: f32| {
        let p = ray.origin + direction * t;
        p.y <= ground_at(p)
    };
    // Marche le long du rayon (en s'arrêtant sur les bâtiments), puis affine par dichotomie.
    let step = 0.25;
    let mut t = 0.0;
    while t < 900.0 {
        let p = ray.origin + direction * (t + step);
        let tile = world::world_to_tile(map, p);
        if let Some(building) = sim.buildings.at(tile)
            && p.y <= ground_at(p) + building_height(building.kind)
        {
            return Some(tile);
        }
        if below_ground(t + step) {
            let (mut lo, mut hi) = (t, t + step);
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                if below_ground(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let tile = world::world_to_tile(map, ray.origin + direction * hi);
            let size = map.size as i32;
            let inside = (0..size).contains(&tile.x) && (0..size).contains(&tile.y);
            return inside.then_some(tile);
        }
        t += step;
    }
    None
}

fn update_cursor(
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    sim: Res<Sim>,
    curvature: Res<CurrentCurvature>,
    blockers: Query<&Hovered, With<BlocksWorldInput>>,
    mut state: ResMut<BuildState>,
) {
    let cursor = match window.cursor_position() {
        Some(position) if !pointer_over_ui(&blockers) => {
            pick_tile(position, *camera, &sim, curvature.params)
        }
        _ => None,
    };
    if state.cursor != cursor {
        state.cursor = cursor;
    }
}

/// Cases d'un tracé en « L » (d'abord horizontal, puis vertical), avec l'orientation de chacune.
fn path(from: Tile, to: Tile, default_dir: Dir) -> Vec<(Tile, Dir)> {
    let mut tiles = vec![from];
    let mut current = from;
    while current.x != to.x {
        current.x += (to.x - current.x).signum();
        tiles.push(current);
    }
    while current.y != to.y {
        current.y += (to.y - current.y).signum();
        tiles.push(current);
    }
    let mut result = Vec::with_capacity(tiles.len());
    let mut dir = default_dir;
    for (i, &tile) in tiles.iter().enumerate() {
        if let Some(&next) = tiles.get(i + 1) {
            dir = Dir::between(tile, next).unwrap_or(dir);
        }
        result.push((tile, dir));
    }
    result
}

/// Cases concernées par l'action en cours (une seule, ou une ligne pendant un glisser).
fn planned_tiles(state: &BuildState) -> Vec<(Tile, Dir)> {
    let Some(cursor) = state.cursor else {
        return Vec::new();
    };
    match (state.tool, state.drag_start) {
        (Some(Tool::Build(BuildingKind::Conveyor) | Tool::Demolish), Some(start)) => {
            path(start, cursor, state.dir)
        }
        _ => vec![(cursor, state.dir)],
    }
}

fn is_tower(sim: &Sim, tile: Tile) -> bool {
    sim.buildings
        .at(tile)
        .is_some_and(|b| b.kind == BuildingKind::Tower)
}

/// Envoie à la simulation les ordres pour les cases prévues. Renvoie le premier refus.
fn issue(sim: &mut Sim, tool: Tool, tiles: &[(Tile, Dir)]) -> Option<Tr> {
    let mut budget = sim.stock;
    let mut first_error = None;
    for &(tile, dir) in tiles {
        match tool {
            Tool::Build(kind) => {
                let check = sim.check_build(kind, tile).and_then(|_| {
                    (budget >= kind.cost())
                        .then_some(())
                        .ok_or(BuildError::NotEnoughCrystals)
                });
                match check {
                    Ok(()) => {
                        budget -= kind.cost();
                        sim.queue(Command::Build { kind, tile, dir });
                    }
                    Err(error) => {
                        first_error.get_or_insert(Tr::Error(error));
                    }
                }
            }
            Tool::Demolish => sim.queue(Command::Demolish { tile }),
            Tool::Socket(_) | Tool::Unsocket if !is_tower(sim, tile) => {
                first_error.get_or_insert(Tr::NeedsTower);
            }
            Tool::Socket(gem) => sim.queue(Command::Socket { tile, gem }),
            Tool::Unsocket => sim.queue(Command::Unsocket { tile }),
        }
    }
    first_error
}

/// Après un clic : mémorise le refus éventuel ; un sertissage réussi range l'outil.
fn after_issue(state: &mut BuildState, error: Option<Tr>, now: f32) {
    match error {
        Some(error) => state.error = Some((error, now + 2.5)),
        None if matches!(state.tool, Some(Tool::Socket(_))) => state.tool = None,
        None => {}
    }
}

fn mouse_build(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    real_time: Res<Time<Real>>,
    mut sim: ResMut<Sim>,
    mut state: ResMut<BuildState>,
    mut right_press: Local<Option<Vec2>>,
) {
    let Some(tool) = state.tool else {
        state.drag_start = None;
        return;
    };
    if buttons.just_pressed(MouseButton::Left) {
        state.drag_start = state.cursor;
    }
    if buttons.just_released(MouseButton::Left) {
        if state.drag_start.is_some() {
            let tiles = planned_tiles(&state);
            // Après une ligne, les clics suivants gardent l'orientation de la ligne.
            if let Some(&(_, dir)) = tiles.last()
                && tiles.len() > 1
            {
                state.dir = dir;
            }
            let error = issue(&mut sim, tool, &tiles);
            after_issue(&mut state, error, real_time.elapsed_secs());
        }
        state.drag_start = None;
    }

    // Un clic droit sans glisser annule l'outil (glisser sert à orienter la caméra).
    if buttons.just_pressed(MouseButton::Right) {
        *right_press = window.cursor_position();
    }
    if buttons.just_released(MouseButton::Right) {
        let moved = match (*right_press, window.cursor_position()) {
            (Some(a), Some(b)) => a.distance(b),
            _ => f32::MAX,
        };
        if moved < 6.0 {
            state.tool = None;
        }
    }
}

/// Sur écran tactile : un appui bref sans glisser pose le bâtiment.
fn touch_build(
    touches: Res<Touches>,
    camera: Single<(&Camera, &GlobalTransform)>,
    real_time: Res<Time<Real>>,
    curvature: Res<CurrentCurvature>,
    blockers: Query<&Hovered, With<BlocksWorldInput>>,
    mut sim: ResMut<Sim>,
    mut state: ResMut<BuildState>,
) {
    let Some(tool) = state.tool else {
        return;
    };
    for touch in touches.iter_just_released() {
        let is_tap = touch.start_position().distance(touch.position()) < 12.0;
        if !is_tap || pointer_over_ui(&blockers) || touches.iter().count() > 0 {
            continue;
        }
        if let Some(tile) = pick_tile(touch.position(), *camera, &sim, curvature.params) {
            let dir = state.dir;
            let error = issue(&mut sim, tool, &[(tile, dir)]);
            after_issue(&mut state, error, real_time.elapsed_secs());
        }
    }
}

/// Aperçu : contour vert (possible) ou rouge (refusé) et flèche d'orientation.
fn draw_preview(
    mut gizmos: Gizmos,
    state: Res<BuildState>,
    sim: Res<Sim>,
    curvature: Res<CurrentCurvature>,
) {
    let Some(tool) = state.tool else {
        return;
    };
    let map = &sim.map;
    let mut budget = sim.stock;
    for (tile, dir) in planned_tiles(&state) {
        let ok = match tool {
            Tool::Build(kind) => {
                let ok = sim.check_build(kind, tile).is_ok() && budget >= kind.cost();
                if ok {
                    budget -= kind.cost();
                }
                ok
            }
            Tool::Demolish => sim
                .buildings
                .at(tile)
                .is_some_and(|b| b.kind != BuildingKind::Hq),
            Tool::Socket(_) | Tool::Unsocket => is_tower(&sim, tile),
        };
        let color = match (tool, ok) {
            (Tool::Demolish, true) => LinearRgba::rgb(4.0, 0.6, 0.3),
            (_, true) => LinearRgba::rgb(0.4, 3.0, 1.2),
            (_, false) => LinearRgba::rgb(3.0, 0.25, 0.25),
        };

        let center = world::tile_to_world(map, tile) + Vec3::Y * 0.08;
        let lift = |p: Vec3| bend(p, curvature.params);
        let h = world::TILE_SIZE * 0.47;
        let corners = [
            center + Vec3::new(-h, 0.0, -h),
            center + Vec3::new(h, 0.0, -h),
            center + Vec3::new(h, 0.0, h),
            center + Vec3::new(-h, 0.0, h),
            center + Vec3::new(-h, 0.0, -h),
        ];
        gizmos.linestrip(corners.map(lift), color);

        match tool {
            Tool::Build(BuildingKind::Tower | BuildingKind::Hq)
            | Tool::Socket(_)
            | Tool::Unsocket => {}
            Tool::Build(_) => {
                let (dx, dy) = dir.offset();
                let forward = Vec3::new(dx as f32, 0.0, dy as f32);
                gizmos.arrow(
                    lift(center - forward * 0.3),
                    lift(center + forward * 0.4),
                    color,
                );
            }
            Tool::Demolish => {
                gizmos.line(lift(corners[0]), lift(corners[2]), color);
                gizmos.line(lift(corners[1]), lift(corners[3]), color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conveyor_path_follows_the_drag() {
        let tiles = path(Tile::new(0, 0), Tile::new(2, 1), Dir::North);
        assert_eq!(
            tiles,
            vec![
                (Tile::new(0, 0), Dir::East),
                (Tile::new(1, 0), Dir::East),
                (Tile::new(2, 0), Dir::South),
                (Tile::new(2, 1), Dir::South),
            ]
        );
        assert_eq!(
            path(Tile::new(5, 5), Tile::new(5, 5), Dir::West),
            vec![(Tile::new(5, 5), Dir::West)]
        );
    }
}
