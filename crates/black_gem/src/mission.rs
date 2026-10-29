//! Écran de départ : choix du format d'expédition (selon le temps disponible) et de la planète.

use bevy::{
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_sim::{Format, Simulation, rng::mix64};

use crate::{
    build::BlocksWorldInput,
    i18n::Language,
    sim::{FIRST_SEED, NewPlanet, Sim},
};

#[derive(Resource)]
pub struct MissionMenu {
    pub open: bool,
    pub format: Format,
    pub seed: u64,
}

impl Default for MissionMenu {
    fn default() -> Self {
        Self {
            open: true,
            format: Format::Expedition,
            seed: FIRST_SEED,
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
            .add_systems(Update, (update_menu, launch_with_enter));
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
                    .observe(|_: On<PointerClick>, mut menu: ResMut<MissionMenu>, mut sim: ResMut<Sim>, mut new_planet: MessageWriter<NewPlanet>| {
                        launch(&mut menu, &mut sim, &mut new_planet);
                    })
                    .with_children(|b| {
                        b.spawn((MenuLabel::Land, Text::default(), style(&font, 22.0, Color::WHITE)));
                    });
            });
        });
}

/// Génère la planète choisie et lance l'expédition.
fn launch(menu: &mut MissionMenu, sim: &mut Sim, new_planet: &mut MessageWriter<NewPlanet>) {
    sim.0 = Simulation::with_format(menu.seed, menu.format);
    new_planet.write(NewPlanet);
    menu.open = false;
}

fn launch_with_enter(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MissionMenu>,
    mut sim: ResMut<Sim>,
    mut new_planet: MessageWriter<NewPlanet>,
) {
    if menu.open && keys.just_pressed(KeyCode::Enter) {
        launch(&mut menu, &mut sim, &mut new_planet);
    }
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
