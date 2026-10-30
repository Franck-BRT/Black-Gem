//! Écran de départ : choix du format d'expédition (selon le temps disponible) et de la planète.

use bevy::{
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_sim::{ExpeditionConfig, Format, Simulation, rng::mix64};

use crate::{
    build::BlocksWorldInput,
    i18n::Language,
    palette::gem_ui,
    profile::{Profile, RESEARCH, all_color_sets},
    sim::{FIRST_SEED, NewPlanet, Sim},
};

#[derive(Resource)]
pub struct MissionMenu {
    pub open: bool,
    pub format: Format,
    pub seed: u64,
    pub tab: Tab,
}

/// Onglets de l'Arche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Component)]
pub enum Tab {
    #[default]
    Expedition,
    Research,
    Codex,
    Stats,
}

impl Default for MissionMenu {
    fn default() -> Self {
        Self {
            open: true,
            format: Format::Expedition,
            seed: FIRST_SEED,
            tab: Tab::Expedition,
        }
    }
}

/// Condition d'exécution : une expédition est en cours (le menu est fermé).
pub fn playing(menu: Res<MissionMenu>) -> bool {
    !menu.open
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct FormatCard(Format);

#[derive(Component)]
struct FormatText(Format);

#[derive(Component)]
struct PlanetText;

#[derive(Component)]
struct GoalText;

/// Page d'un onglet (affichée seulement quand l'onglet est actif).
#[derive(Component)]
struct TabPage(Tab);

#[derive(Component)]
struct TabButton(Tab);

#[derive(Component)]
struct TabLabel(Tab);

#[derive(Component)]
struct ShardsText;

#[derive(Component)]
struct ResearchList;

#[derive(Component)]
struct CodexList;

#[derive(Component)]
struct StatsText;

#[derive(Resource)]
struct MenuFont(Handle<Font>);

#[derive(Component, Clone, Copy)]
enum MenuLabel {
    Title,
    Previous,
    Next,
    Random,
    Land,
}

const CARD: Color = Color::srgba(0.1, 0.08, 0.16, 0.95);
const CARD_SELECTED: Color = Color::srgba(0.42, 0.2, 0.72, 0.98);

const UI_FONT: &[u8] = include_bytes!("../assets/fonts/Exo2.ttf");

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MissionMenu>()
            .add_systems(Startup, spawn_menu)
            .add_systems(
                Update,
                (
                    update_menu,
                    update_tabs,
                    update_research,
                    update_codex,
                    update_stats,
                    launch_with_enter,
                ),
            );
    }
}

fn style(font: &Handle<Font>, size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font: font.clone().into(),
            font_size: size.into(),
            ..default()
        },
        TextColor(color),
    )
}

fn small_button() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

fn spawn_menu(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = fonts.add(Font::from_bytes(UI_FONT.to_vec()));
    commands.insert_resource(MenuFont(font.clone()));
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.55)),
            // Au-dessus de toute l'interface du jeu.
            GlobalZIndex(10),
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(16.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    max_width: Val::Percent(96.0),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.01, 0.05, 0.92)),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("BLACK GEM"), style(&font, 40.0, Color::srgb(0.78, 0.55, 1.0))));
                // Onglets de l'Arche et réserve d'éclats noirs.
                panel
                    .spawn(Node {
                        column_gap: Val::Px(8.0),
                        align_items: AlignItems::Center,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        for tab in [Tab::Expedition, Tab::Research, Tab::Codex, Tab::Stats] {
                            row.spawn((TabButton(tab), small_button(), BackgroundColor(CARD)))
                                .observe(move |_: On<PointerClick>, mut menu: ResMut<MissionMenu>| {
                                    menu.tab = tab;
                                })
                                .with_children(|b| {
                                    b.spawn((TabLabel(tab), Text::default(), style(&font, 16.0, Color::WHITE)));
                                });
                        }
                        row.spawn((
                            ShardsText,
                            Text::default(),
                            style(&font, 17.0, Color::srgb(0.8, 0.6, 1.0)),
                            Node {
                                margin: UiRect::left(Val::Px(16.0)),
                                ..default()
                            },
                        ));
                    });

                // Pages des autres onglets.
                panel
                    .spawn((
                        TabPage(Tab::Research),
                        ResearchList,
                        Node {
                            display: Display::None,
                            width: Val::Px(820.0),
                            max_width: Val::Percent(100.0),
                            column_gap: Val::Px(10.0),
                            row_gap: Val::Px(10.0),
                            flex_wrap: FlexWrap::Wrap,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                    ));
                panel.spawn((
                    TabPage(Tab::Codex),
                    CodexList,
                    Node {
                        display: Display::None,
                        width: Val::Px(820.0),
                        max_width: Val::Percent(100.0),
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                ));
                panel.spawn((
                    TabPage(Tab::Stats),
                    StatsText,
                    Text::default(),
                    style(&font, 17.0, Color::srgb(0.9, 0.9, 0.95)),
                    Node {
                        display: Display::None,
                        ..default()
                    },
                ));

                // Page « Expédition ».
                panel
                    .spawn((
                        TabPage(Tab::Expedition),
                        Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(16.0),
                            ..default()
                        },
                    ))
                    .with_children(|panel| {
                panel.spawn((MenuLabel::Title, Text::default(), style(&font, 20.0, Color::WHITE)));

                // Les formats, côte à côte.
                panel
                    .spawn(Node {
                        column_gap: Val::Px(12.0),
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        for format in Format::ALL {
                            row.spawn((
                                FormatCard(format),
                                Node {
                                    width: Val::Px(190.0),
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::all(Val::Px(12.0)),
                                    border_radius: BorderRadius::all(Val::Px(8.0)),
                                    ..default()
                                },
                                BackgroundColor(CARD),
                            ))
                            .observe(move |_: On<PointerClick>, mut menu: ResMut<MissionMenu>| {
                                menu.format = format;
                            })
                            .with_children(|card| {
                                card.spawn((
                                    FormatText(format),
                                    Text::default(),
                                    style(&font, 14.0, Color::srgb(0.9, 0.9, 0.95)),
                                ));
                            });
                        }
                    });

                // Choix de la planète.
                panel
                    .spawn(Node {
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        let button = |row: &mut ChildSpawnerCommands, label: MenuLabel| {
                            row.spawn((small_button(), BackgroundColor(CARD)))
                                .observe(move |_: On<PointerClick>, mut menu: ResMut<MissionMenu>, time: Res<Time<Real>>| {
                                    match label {
                                        MenuLabel::Previous => menu.seed = menu.seed.saturating_sub(1).max(1),
                                        MenuLabel::Next => menu.seed += 1,
                                        MenuLabel::Random => {
                                            let entropy = time.elapsed().as_nanos() as u64;
                                            menu.seed = mix64(entropy ^ menu.seed) % 1_000_000 + 1;
                                        }
                                        _ => {}
                                    }
                                })
                                .with_children(|b| {
                                    b.spawn((label, Text::default(), style(&font, 16.0, Color::WHITE)));
                                });
                        };
                        button(row, MenuLabel::Previous);
                        row.spawn((PlanetText, Text::default(), style(&font, 18.0, Color::WHITE)));
                        button(row, MenuLabel::Next);
                        button(row, MenuLabel::Random);
                    });

                panel.spawn((
                    GoalText,
                    Text::default(),
                    style(&font, 15.0, Color::srgb(0.8, 0.82, 0.9)),
                    TextLayout::justify(Justify::Center),
                ));

                panel
                    .spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(36.0), Val::Px(12.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(CARD_SELECTED),
                    ))
                    .observe(|_: On<PointerClick>, mut menu: ResMut<MissionMenu>, mut sim: ResMut<Sim>, profile: Res<Profile>, mut new_planet: MessageWriter<NewPlanet>| {
                        launch(&mut menu, &mut sim, &profile, &mut new_planet);
                    })
                    .with_children(|b| {
                        b.spawn((MenuLabel::Land, Text::default(), style(&font, 22.0, Color::WHITE)));
                    });
                    });
            });
        });
}

/// Génère la planète choisie et lance l'expédition.
fn launch(
    menu: &mut MissionMenu,
    sim: &mut Sim,
    profile: &Profile,
    new_planet: &mut MessageWriter<NewPlanet>,
) {
    // Les recherches de l'Arche s'appliquent à toute l'expédition.
    let config = ExpeditionConfig {
        bonuses: profile.bonuses(),
        ..menu.format.config()
    };
    sim.0 = Simulation::with_config(menu.seed, config);
    new_planet.write(NewPlanet);
    menu.open = false;
}

fn launch_with_enter(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MissionMenu>,
    mut sim: ResMut<Sim>,
    profile: Res<Profile>,
    mut new_planet: MessageWriter<NewPlanet>,
) {
    if menu.open && menu.tab == Tab::Expedition && keys.just_pressed(KeyCode::Enter) {
        launch(&mut menu, &mut sim, &profile, &mut new_planet);
    }
}

fn update_tabs(
    menu: Res<MissionMenu>,
    profile: Res<Profile>,
    language: Res<Language>,
    mut pages: Query<(&TabPage, &mut Node)>,
    mut buttons: Query<(&TabButton, &mut BackgroundColor)>,
    mut labels: Query<(&TabLabel, &mut Text), Without<ShardsText>>,
    mut shards: Single<&mut Text, With<ShardsText>>,
) {
    if !menu.is_changed() && !language.is_changed() && !profile.is_changed() {
        return;
    }
    for (page, mut node) in &mut pages {
        node.display = if page.0 == menu.tab {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (button, mut background) in &mut buttons {
        background.0 = if button.0 == menu.tab {
            CARD_SELECTED
        } else {
            CARD
        };
    }
    let french = *language == Language::French;
    for (label, mut text) in &mut labels {
        text.0 = match (label.0, french) {
            (Tab::Expedition, true) => "Expédition",
            (Tab::Expedition, false) => "Expedition",
            (Tab::Research, true) => "Recherche",
            (Tab::Research, false) => "Research",
            (Tab::Codex, _) => "Codex",
            (Tab::Stats, true) => "Statistiques",
            (Tab::Stats, false) => "Statistics",
        }
        .to_string();
    }
    shards.0 = if french {
        format!("Éclats noirs : {}", profile.shards)
    } else {
        format!("Black shards: {}", profile.shards)
    };
}

/// Arbre de recherche : chaque carte montre l'effet, le coût et l'état (acquise, disponible,
/// verrouillée par un prérequis). Un clic achète.
fn update_research(
    mut commands: Commands,
    profile: Res<Profile>,
    language: Res<Language>,
    font: Res<MenuFont>,
    list: Single<Entity, With<ResearchList>>,
) {
    if !profile.is_changed() && !language.is_changed() {
        return;
    }
    let lang = *language;
    let french = lang == Language::French;
    commands.entity(*list).despawn_children();
    commands.entity(*list).with_children(|grid| {
        for research in &RESEARCH {
            let owned = profile.has(research.id);
            let unlocked = research.requires.iter().all(|r| profile.has(r));
            let affordable = profile.shards >= research.cost;
            let (background, status) = match (owned, unlocked, affordable) {
                (true, _, _) => (
                    Color::srgba(0.12, 0.35, 0.22, 0.95),
                    if french {
                        "Acquise".to_string()
                    } else {
                        "Acquired".to_string()
                    },
                ),
                (false, false, _) => {
                    let names: Vec<&str> = research
                        .requires
                        .iter()
                        .filter_map(|id| RESEARCH.iter().find(|r| r.id == *id))
                        .map(|r| r.name(lang))
                        .collect();
                    (
                        Color::srgba(0.08, 0.07, 0.1, 0.95),
                        if french {
                            format!("Requiert : {}", names.join(", "))
                        } else {
                            format!("Requires: {}", names.join(", "))
                        },
                    )
                }
                (false, true, true) => (
                    CARD_SELECTED,
                    if french {
                        format!("{} éclats — acheter", research.cost)
                    } else {
                        format!("{} shards — buy", research.cost)
                    },
                ),
                (false, true, false) => (
                    CARD,
                    if french {
                        format!("{} éclats", research.cost)
                    } else {
                        format!("{} shards", research.cost)
                    },
                ),
            };
            let id = research.id;
            grid.spawn((
                Node {
                    width: Val::Px(190.0),
                    min_height: Val::Px(96.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(background),
            ))
            .observe(move |_: On<PointerClick>, mut profile: ResMut<Profile>| {
                profile.buy(id);
            })
            .with_children(|card| {
                card.spawn((
                    Text::new(research.name(lang)),
                    style(&font.0, 15.0, Color::WHITE),
                ));
                card.spawn((
                    Text::new(research.description(lang)),
                    style(&font.0, 13.0, Color::srgb(0.8, 0.82, 0.9)),
                ));
                card.spawn((
                    Text::new(status),
                    style(&font.0, 13.0, Color::srgb(1.0, 0.85, 0.5)),
                ));
            });
        }
    });
}

/// Codex : les 31 combinaisons de couleurs, découvertes (avec le meilleur grade) ou non.
fn update_codex(
    mut commands: Commands,
    profile: Res<Profile>,
    language: Res<Language>,
    font: Res<MenuFont>,
    list: Single<Entity, With<CodexList>>,
) {
    if !profile.is_changed() && !language.is_changed() {
        return;
    }
    let french = *language == Language::French;
    let sets = all_color_sets();
    let found = profile.codex.len();
    commands.entity(*list).despawn_children();
    commands.entity(*list).with_children(|grid| {
        grid.spawn((
            Text::new(if french {
                format!("Gemmes découvertes : {found} / {}", sets.len())
            } else {
                format!("Gems discovered: {found} / {}", sets.len())
            }),
            style(&font.0, 16.0, Color::WHITE),
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
        ));
        for set in sets {
            let key = set
                .iter()
                .map(|c| format!("{c:?}"))
                .collect::<Vec<_>>()
                .join("+");
            let best = profile.codex.get(&key).copied();
            grid.spawn((
                Node {
                    width: Val::Px(92.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    column_gap: Val::Px(3.0),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(if best.is_some() {
                    CARD
                } else {
                    Color::srgba(0.05, 0.04, 0.07, 0.9)
                }),
            ))
            .with_children(|cell| {
                for color in &set {
                    let swatch = if best.is_some() {
                        gem_ui(*color)
                    } else {
                        Color::srgb(0.25, 0.25, 0.3)
                    };
                    cell.spawn((
                        Node {
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            border_radius: BorderRadius::all(Val::Px(5.0)),
                            ..default()
                        },
                        BackgroundColor(swatch),
                    ));
                }
                cell.spawn((
                    Text::new(best.map_or("?".to_string(), |g| format!("G{g}"))),
                    style(&font.0, 13.0, Color::WHITE),
                ));
            });
        }
    });
}

fn update_stats(
    profile: Res<Profile>,
    language: Res<Language>,
    mut text: Single<&mut Text, With<StatsText>>,
) {
    if !profile.is_changed() && !language.is_changed() {
        return;
    }
    let french = *language == Language::French;
    let stats = &profile.stats;
    let mut lines = vec![
        if french {
            format!(
                "Expéditions : {}   Victoires : {}",
                stats.expeditions, stats.victories
            )
        } else {
            format!(
                "Expeditions: {}   Victories: {}",
                stats.expeditions, stats.victories
            )
        },
        if french {
            format!("Ennemis détruits : {}", stats.kills)
        } else {
            format!("Enemies destroyed: {}", stats.kills)
        },
        if french {
            format!(
                "Recherches : {} / {}   Programmes enregistrés : {}",
                profile.research.len(),
                RESEARCH.len(),
                profile.scripts.len()
            )
        } else {
            format!(
                "Research: {} / {}   Saved programs: {}",
                profile.research.len(),
                RESEARCH.len(),
                profile.scripts.len()
            )
        },
        String::new(),
        if french {
            "Meilleurs temps :".to_string()
        } else {
            "Best times:".to_string()
        },
    ];
    for format in [Format::Scout, Format::Expedition, Format::GreatDrill] {
        let name = describe(format, *language)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let time = profile
            .stats
            .best_seconds
            .get(&format!("{format:?}"))
            .map_or("—".to_string(), |s| format!("{}:{:02}", s / 60, s % 60));
        lines.push(format!("  {name} : {time}"));
    }
    text.0 = lines.join("\n");
}

/// Description d'un format : nom, durée, carte, journées, paliers.
fn describe(format: Format, lang: Language) -> String {
    let config = format.config();
    let french = lang == Language::French;
    let (name, duration) = match (format, french) {
        (Format::Scout, true) => ("Éclaireur", "~15 min"),
        (Format::Scout, false) => ("Scout", "~15 min"),
        (Format::Expedition, true) => ("Expédition", "30 à 45 min"),
        (Format::Expedition, false) => ("Expedition", "30 to 45 min"),
        (Format::GreatDrill, true) => ("Grand forage", "60 à 90 min"),
        (Format::GreatDrill, false) => ("Great drill", "60 to 90 min"),
        (Format::Colony, true) => ("Colonie", "sans limite"),
        (Format::Colony, false) => ("Colony", "no time limit"),
    };
    let day_minutes = config.ticks_per_day / (60 * gem_sim::TICKS_PER_SECOND);
    let stages = match (config.stages, french) {
        (0, true) => "pas de forage : survivre".to_string(),
        (0, false) => "no drilling: survive".to_string(),
        (n, true) => format!("{n} paliers de forage"),
        (n, false) => format!("{n} drilling stages"),
    };
    let (map, day) = if french {
        ("Carte", "journée")
    } else {
        ("Map", "day")
    };
    format!(
        "{name}\n{duration}\n\n{map} {size} × {size}\n{day} {day_minutes} min\n{stages}",
        size = config.map_size
    )
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_menu(
    menu: Res<MissionMenu>,
    language: Res<Language>,
    mut root: Single<&mut Visibility, With<MenuRoot>>,
    mut cards: Query<(&FormatCard, &mut BackgroundColor)>,
    mut texts: Query<(&FormatText, &mut Text), (Without<PlanetText>, Without<GoalText>)>,
    mut planet: Single<&mut Text, (With<PlanetText>, Without<GoalText>, Without<MenuLabel>)>,
    mut goal: Single<&mut Text, (With<GoalText>, Without<MenuLabel>)>,
    mut labels: Query<
        (&MenuLabel, &mut Text),
        (Without<FormatText>, Without<PlanetText>, Without<GoalText>),
    >,
) {
    **root = if menu.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !menu.is_changed() && !language.is_changed() {
        return;
    }
    let lang = *language;
    let french = lang == Language::French;
    for (card, mut background) in &mut cards {
        background.0 = if card.0 == menu.format {
            CARD_SELECTED
        } else {
            CARD
        };
    }
    for (text, mut content) in &mut texts {
        content.0 = describe(text.0, lang);
    }
    planet.0 = if french {
        format!("Planète n° {}", menu.seed)
    } else {
        format!("Planet #{}", menu.seed)
    };
    goal.0 = match (menu.format, french) {
        (Format::Colony, true) => "Bâtis ta colonie et survis aux nuits, aussi longtemps que possible.".into(),
        (Format::Colony, false) => "Build your colony and survive the nights, as long as you can.".into(),
        (_, true) => "Construis le Grand foreur sur le Cœur et alimente-le en cristaux.\nChaque palier réveille un boss. À 100 %, la Gemme Noire est extraite :\ntiens jusqu'au décollage.".into(),
        (_, false) => "Build the Great drill on the Core and feed it crystals.\nEach stage wakes up a boss. At 100%, the Black Gem is extracted:\nhold until lift-off.".into(),
    };
    for (label, mut text) in &mut labels {
        text.0 = match (label, french) {
            (MenuLabel::Title, true) => "Nouvelle expédition : combien de temps as-tu ?",
            (MenuLabel::Title, false) => "New expedition: how much time do you have?",
            (MenuLabel::Previous, _) => "<",
            (MenuLabel::Next, _) => ">",
            (MenuLabel::Random, true) => "Au hasard",
            (MenuLabel::Random, false) => "Random",
            (MenuLabel::Land, true) => "Atterrir",
            (MenuLabel::Land, false) => "Land",
        }
        .to_string();
    }
}
