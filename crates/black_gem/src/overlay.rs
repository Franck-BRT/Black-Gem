//! Informations par-dessus le monde : barres de vie (ennemis blessés, boss, bâtiments
//! endommagés), bandeau d'alertes (« Le QG est attaqué ! ») et liseré rouge quand le QG
//! est frappé.

use bevy::prelude::*;
use gem_sim::{BuildingKind, SimEvent};

use crate::{
    camera::RtsCamera,
    curvature::{CurrentCurvature, bend},
    i18n::{Language, Tr},
    mission::{MissionMenu, playing},
    sim::{Sim, SimEvents},
    world,
};

/// Barres affichées au plus (les plus proches de la caméra d'abord… en pratique, l'ordre de
/// la simulation suffit).
const MAX_BARS: usize = 160;
const ALERT_SECONDS: f32 = 5.0;

#[derive(Component)]
struct Bar;

#[derive(Component)]
struct BarFill;

#[derive(Component)]
struct AlertText;

#[derive(Component)]
struct DamageFlash;

#[derive(Resource, Default)]
struct Alerts {
    lines: Vec<(String, f32)>,
    /// Pour ne pas répéter la même alerte en boucle.
    last_hq: f32,
    last_hit: f32,
    flash: f32,
}

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Alerts>()
            .add_systems(Startup, spawn_overlay)
            .add_systems(PostUpdate, health_bars.after(TransformSystems::Propagate))
            .add_systems(Update, (collect_alerts.run_if(playing), show_alerts));
    }
}

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(150.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            row.spawn((
                AlertText,
                Text::default(),
                TextFont {
                    font_size: 22.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.5)),
                TextLayout::justify(Justify::Center),
                Pickable::IGNORE,
            ));
        });
    commands.spawn((
        DamageFlash,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            border: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BorderColor::all(Color::NONE),
        Pickable::IGNORE,
    ));
}

/// Hauteur de la barre au-dessus du sol, et sa largeur en pixels.
fn building_bar(kind: BuildingKind) -> (f32, f32) {
    match kind {
        BuildingKind::Hq => (4.6, 90.0),
        BuildingKind::CoreDrill => (5.0, 90.0),
        BuildingKind::Tower => (3.2, 40.0),
        BuildingKind::Conveyor => (0.8, 24.0),
        _ => (2.2, 36.0),
    }
}

/// Une barre : son nœud et ses enfants (le remplissage).
type BarParts<'a> = (Entity, &'a mut Node, &'a Children);

#[allow(clippy::too_many_arguments)]
fn health_bars(
    mut commands: Commands,
    sim: Res<Sim>,
    menu: Res<MissionMenu>,
    fixed: Res<Time<Fixed>>,
    curvature: Res<CurrentCurvature>,
    fog: Res<crate::fog::FogOfWar>,
    ui_scale: Res<UiScale>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    mut bars: Query<BarParts, (With<Bar>, Without<BarFill>)>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<BarFill>>,
) {
    let (camera, camera_transform) = *camera;
    let mut wanted: Vec<(Vec3, f32, f32)> = Vec::new();
    if !menu.open {
        let alpha = fixed.overstep_fraction();
        for enemy in &sim.enemies {
            if (enemy.hp >= enemy.max_hp && !enemy.boss) || !fog.visible_pos(enemy.pos) {
                continue;
            }
            let from = world::pos_to_world(&sim.map, enemy.prev_pos);
            let to = world::pos_to_world(&sim.map, enemy.pos);
            let (height, width) = if enemy.boss { (3.4, 64.0) } else { (1.6, 26.0) };
            let fraction = enemy.hp.max(0) as f32 / enemy.max_hp.max(1) as f32;
            wanted.push((from.lerp(to, alpha) + Vec3::Y * height, width, fraction));
        }
        // Le héros garde sa barre ; les soldats, quand ils sont blessés.
        for unit in &sim.units {
            let hero = unit.kind == gem_sim::UnitKind::Hero;
            if unit.hp >= unit.max_hp && !hero {
                continue;
            }
            let from = world::pos_to_world(&sim.map, unit.prev_pos);
            let to = world::pos_to_world(&sim.map, unit.pos);
            let (height, width) = if hero { (2.1, 40.0) } else { (1.5, 24.0) };
            let fraction = unit.hp.max(0) as f32 / unit.max_hp.max(1) as f32;
            wanted.push((from.lerp(to, alpha) + Vec3::Y * height, width, fraction));
        }
        for (i, nest) in sim.nests.iter().enumerate() {
            let hp = sim.nest_hp[i];
            if hp <= 0 || hp >= sim.nest_max_hp || !sim.is_explored(*nest) {
                continue;
            }
            let at = world::tile_to_world(&sim.map, *nest) + Vec3::Y * 4.0;
            wanted.push((at, 80.0, hp as f32 / sim.nest_max_hp.max(1) as f32));
        }
        for building in sim.buildings.iter() {
            if building.hp >= building.max_hp {
                continue;
            }
            let (height, width) = building_bar(building.kind);
            let at = world::pos_to_world(&sim.map, building.center()) + Vec3::Y * height;
            let fraction = building.hp.max(0) as f32 / building.max_hp.max(1) as f32;
            wanted.push((at, width, fraction));
        }
    }
    let mut existing: Vec<(Entity, Mut<Node>, &Children)> = bars.iter_mut().collect();
    let mut shown = 0;
    for (point, width, fraction) in wanted.into_iter().take(MAX_BARS) {
        let bent = bend(point, curvature.params);
        let Ok(screen) = camera.world_to_viewport(camera_transform, bent) else {
            continue;
        };
        // Les positions de l'interface sont multipliées par l'échelle de l'interface.
        let screen = screen / ui_scale.0.max(0.01);
        let color = if fraction > 0.6 {
            Color::srgb(0.35, 0.95, 0.45)
        } else if fraction > 0.3 {
            Color::srgb(1.0, 0.8, 0.25)
        } else {
            Color::srgb(1.0, 0.3, 0.25)
        };
        if shown < existing.len() {
            let (_, node, children) = &mut existing[shown];
            node.display = Display::Flex;
            node.left = Val::Px(screen.x - width / 2.0);
            node.top = Val::Px(screen.y);
            node.width = Val::Px(width);
            if let Some(&fill) = children.first()
                && let Ok((mut fill_node, mut fill_color)) = fills.get_mut(fill)
            {
                fill_node.width = Val::Percent(fraction * 100.0);
                fill_color.0 = color;
            }
        } else {
            commands
                .spawn((
                    Bar,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(screen.x - width / 2.0),
                        top: Val::Px(screen.y),
                        width: Val::Px(width),
                        height: Val::Px(5.0),
                        padding: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                    GlobalZIndex(-1),
                    Pickable::IGNORE,
                ))
                .with_children(|bar| {
                    bar.spawn((
                        BarFill,
                        Node {
                            width: Val::Percent(fraction * 100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(color),
                        Pickable::IGNORE,
                    ));
                });
        }
        shown += 1;
    }
    for (_, node, _) in existing.iter_mut().skip(shown) {
        if node.display != Display::None {
            node.display = Display::None;
        }
    }
}

/// Accord en français : « tour détruite », « convoyeur détruit ».
fn feminine(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Drill
        | BuildingKind::Cutter
        | BuildingKind::Tower
        | BuildingKind::PowerPlant
        | BuildingKind::Barracks => "e",
        _ => "",
    }
}

fn collect_alerts(
    sim: Res<Sim>,
    events: Res<SimEvents>,
    language: Res<Language>,
    time: Res<Time<Real>>,
    mut alerts: ResMut<Alerts>,
) {
    let now = time.elapsed_secs();
    let french = *language == Language::French;
    for event in events.iter() {
        let text = match *event {
            SimEvent::NightFalls { wave } => Some(if french {
                format!("La nuit tombe — vague {wave}")
            } else {
                format!("Night falls — wave {wave}")
            }),
            SimEvent::StageReached { stage } => Some(if french {
                format!("Palier {stage} franchi : des boss arrivent !")
            } else {
                format!("Stage {stage} reached: bosses incoming!")
            }),
            SimEvent::HqHit => {
                alerts.flash = 1.0;
                if now - alerts.last_hq > 8.0 {
                    alerts.last_hq = now;
                    Some(
                        if french {
                            "Le QG est attaqué !"
                        } else {
                            "The HQ is under attack!"
                        }
                        .to_string(),
                    )
                } else {
                    None
                }
            }
            SimEvent::BuildingHit { kind, .. } => {
                if now - alerts.last_hit > 10.0 {
                    alerts.last_hit = now;
                    let name = language.tr(Tr::Building(kind));
                    Some(if french {
                        format!(
                            "{name} attaqué{} : les ennemis détruisent ce qui leur barre la route",
                            feminine(kind)
                        )
                    } else {
                        format!("{name} under attack: enemies destroy whatever blocks their way")
                    })
                } else {
                    None
                }
            }
            SimEvent::NestDestroyed { .. } => Some(if french {
                format!(
                    "Nid de corruption rasé ! +{} cristaux",
                    gem_sim::NEST_REWARD
                )
            } else {
                format!("Corruption nest razed! +{} crystals", gem_sim::NEST_REWARD)
            }),
            SimEvent::HeroLevelUp { level } => Some(if french {
                format!("Le Forgeron-Ingénieur passe au niveau {level} !")
            } else {
                format!("The Smith-Engineer reaches level {level}!")
            }),
            SimEvent::PowerShortage => Some(
                if french {
                    "Manque d'énergie : tout ralentit. Construis une Centrale (P)."
                } else {
                    "Power shortage: everything slows down. Build a Power plant (P)."
                }
                .to_string(),
            ),
            SimEvent::UnitLost if sim.hero().is_none() && sim.fallen_hero.is_some() => Some(
                if french {
                    "Le héros est tombé : il revient au QG dans 30 s"
                } else {
                    "The hero has fallen: back at the HQ in 30 s"
                }
                .to_string(),
            ),
            SimEvent::BuildingDestroyed { kind, .. } => {
                let name = language.tr(Tr::Building(kind));
                Some(if french {
                    format!("{name} détruit{} !", feminine(kind))
                } else {
                    format!("{name} destroyed!")
                })
            }
            _ => None,
        };
        if let Some(text) = text
            && !alerts.lines.iter().any(|(t, _)| *t == text)
        {
            alerts.lines.push((text, now));
        }
    }
}

fn show_alerts(
    time: Res<Time<Real>>,
    menu: Res<MissionMenu>,
    mut alerts: ResMut<Alerts>,
    mut text: Single<(&mut Text, &mut TextColor), With<AlertText>>,
    mut flash: Single<&mut BorderColor, With<DamageFlash>>,
) {
    let now = time.elapsed_secs();
    alerts.lines.retain(|(_, at)| now - at < ALERT_SECONDS);
    while alerts.lines.len() > 3 {
        alerts.lines.remove(0);
    }
    let content = if menu.open {
        String::new()
    } else {
        alerts
            .lines
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Jamais vide : un texte vide garde une taille nulle quand il se remplit ensuite.
    let content = if content.is_empty() {
        " ".to_string()
    } else {
        content
    };
    if text.0.0 != content {
        text.0.0 = content;
    }
    // Les alertes s'effacent pendant leur dernière seconde.
    let newest = alerts.lines.last().map_or(0.0, |(_, at)| now - at);
    let fade = (ALERT_SECONDS - newest).clamp(0.0, 1.0);
    text.1.0 = Color::srgba(1.0, 0.85, 0.5, fade.max(0.3));
    alerts.flash = (alerts.flash - time.delta_secs() * 1.5).max(0.0);
    **flash = BorderColor::all(Color::srgba(1.0, 0.1, 0.05, alerts.flash * 0.55));
}
