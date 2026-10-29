//! Interface : état de l'expédition, barre de construction, messages et fin de partie.

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_sim::{BuildingKind, Outcome, TICKS_PER_DAY, TICKS_PER_SECOND, WAVES_TO_WIN};

use crate::{
    build::{BlocksWorldInput, BuildState, Tool},
    i18n::{Language, Tr},
    sim::{GameSpeed, SPEEDS, Sim},
};

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct HelpText;

#[derive(Component)]
struct ToastText;

#[derive(Component)]
struct OutcomeTitle;

#[derive(Component)]
struct OutcomeDetail;

#[derive(Component)]
struct OutcomePanel;

/// Bouton de la barre de construction.
#[derive(Component)]
struct ToolButton(Tool);

#[derive(Component)]
struct ToolLabel(Tool);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    update_status,
                    update_help,
                    update_tool_buttons,
                    update_toast,
                    update_outcome,
                ),
            );
    }
}

/// Police de l'interface (Exo 2, licence OFL) : intégrée au binaire, avec tous les accents.
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/Exo2.ttf");

const PANEL_COLOR: Color = Color::srgba(0.02, 0.01, 0.05, 0.6);
const BUTTON_COLOR: Color = Color::srgba(0.12, 0.1, 0.18, 0.85);
const BUTTON_SELECTED: Color = Color::srgba(0.45, 0.2, 0.75, 0.95);

fn text(font: &Handle<Font>, size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font: font.clone().into(),
            font_size: size.into(),
            ..default()
        },
        TextColor(color),
    )
}

fn panel() -> Node {
    Node {
        position_type: PositionType::Absolute,
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.0),
        padding: UiRect::all(Val::Px(10.0)),
        border_radius: BorderRadius::all(Val::Px(6.0)),
        ..default()
    }
}

fn spawn_hud(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = fonts.add(Font::from_bytes(UI_FONT.to_vec()));

    // En haut à gauche : titre et état de l'expédition.
    commands
        .spawn((
            Node {
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                ..panel()
            },
            BackgroundColor(PANEL_COLOR),
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("BLACK GEM"),
                text(&font, 28.0, Color::srgb(0.78, 0.55, 1.0)),
            ));
            parent.spawn((
                StatusText,
                Text::default(),
                text(&font, 15.0, Color::srgb(0.9, 0.92, 0.95)),
            ));
        });

    // En haut à droite : aide des commandes.
    commands.spawn((
        HelpText,
        Text::default(),
        text(&font, 12.0, Color::srgb(0.8, 0.82, 0.88)),
        Node {
            top: Val::Px(12.0),
            right: Val::Px(12.0),
            max_width: Val::Px(560.0),
            ..panel()
        },
        BackgroundColor(PANEL_COLOR),
        Hovered::default(),
        BlocksWorldInput,
    ));

    // En bas : barre de construction (cliquable, utilisable au doigt).
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                ToastText,
                Text::default(),
                text(&font, 17.0, Color::srgb(1.0, 0.55, 0.45)),
            ));
            parent
                .spawn((
                    Node {
                        column_gap: Val::Px(8.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_COLOR),
                    Hovered::default(),
                    BlocksWorldInput,
                ))
                .with_children(|bar| {
                    let tools = [
                        Tool::Build(BuildingKind::Drill),
                        Tool::Build(BuildingKind::Conveyor),
                        Tool::Build(BuildingKind::Tower),
                        Tool::Demolish,
                    ];
                    for tool in tools {
                        bar.spawn((
                            ToolButton(tool),
                            Node {
                                min_width: Val::Px(110.0),
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                justify_content: JustifyContent::Center,
                                border_radius: BorderRadius::all(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(BUTTON_COLOR),
                        ))
                        .observe(move |_: On<PointerClick>, mut state: ResMut<BuildState>| {
                            state.tool = if state.tool == Some(tool) {
                                None
                            } else {
                                Some(tool)
                            };
                        })
                        .with_children(|button| {
                            button.spawn((
                                ToolLabel(tool),
                                Text::default(),
                                text(&font, 15.0, Color::WHITE),
                                TextLayout::justify(Justify::Center),
                            ));
                        });
                    }
                });
        });

    // Au centre : victoire ou défaite.
    commands
        .spawn((
            OutcomePanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(12.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                OutcomeTitle,
                Text::default(),
                text(&font, 72.0, Color::WHITE),
            ));
            parent.spawn((
                OutcomeDetail,
                Text::default(),
                text(&font, 22.0, Color::srgb(0.9, 0.9, 0.95)),
                TextLayout::justify(Justify::Center),
            ));
        });
}

fn format_duration(ticks: u64) -> String {
    let seconds = ticks / TICKS_PER_SECOND as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn update_status(
    sim: Res<Sim>,
    speed: Res<GameSpeed>,
    language: Res<Language>,
    diagnostics: Res<DiagnosticsStore>,
    mut text: Single<&mut Text, With<StatusText>>,
) {
    let lang = *language;
    let day = sim.tick / TICKS_PER_DAY as u64 + 1;
    let minutes_of_day = sim.time_of_day() * 24 * 60 / 1000;
    let period = if sim.is_night() {
        lang.tr(Tr::NightTime)
    } else {
        lang.tr(Tr::DayTime)
    };
    let speed_label = if speed.paused {
        lang.tr(Tr::Paused).to_string()
    } else {
        format!("x{}", SPEEDS[speed.index])
    };
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or(0.0);
    let threat = if sim.is_night() && sim.wave > 0 {
        lang.tr(Tr::NightNow).to_string()
    } else if sim.wave < WAVES_TO_WIN {
        format!(
            "{} {}",
            lang.tr(Tr::NightIn),
            format_duration(sim.ticks_until_nightfall())
        )
    } else {
        String::new()
    };
    let hq = sim.hq();

    let c = lang.colon();
    text.0 = format!(
        "{}\n{} {} · {:02}:{:02} ({})\n\
         {}{c}{}   {}{c}{} / {}\n\
         {} {} / {} · {} {}\n{}\n\
         {}{c}{} · {}{c}{}   {:.0} FPS",
        lang.tr(Tr::Subtitle),
        lang.tr(Tr::Day),
        day,
        minutes_of_day / 60,
        minutes_of_day % 60,
        period,
        lang.tr(Tr::Crystals),
        sim.stock,
        lang.tr(Tr::HqHealth),
        hq.hp.max(0),
        hq.kind.max_hp(),
        lang.tr(Tr::Wave),
        sim.wave,
        WAVES_TO_WIN,
        sim.kills,
        lang.tr(Tr::Kills),
        threat,
        lang.tr(Tr::Seed),
        sim.map.seed,
        lang.tr(Tr::Speed),
        speed_label,
        fps,
    );
}

fn update_help(language: Res<Language>, mut text: Single<&mut Text, With<HelpText>>) {
    if language.is_changed() {
        let lang = *language;
        text.0 = format!("{}\n{}", lang.tr(Tr::Controls), lang.tr(Tr::LanguageName));
    }
}

fn update_tool_buttons(
    state: Res<BuildState>,
    language: Res<Language>,
    mut buttons: Query<(&ToolButton, &mut BackgroundColor)>,
    mut labels: Query<(&ToolLabel, &mut Text)>,
) {
    if !state.is_changed() && !language.is_changed() {
        return;
    }
    for (button, mut background) in &mut buttons {
        background.0 = if state.tool == Some(button.0) {
            BUTTON_SELECTED
        } else {
            BUTTON_COLOR
        };
    }
    for (label, mut text) in &mut labels {
        text.0 = match label.0 {
            Tool::Build(kind) => {
                let key = match kind {
                    BuildingKind::Drill => "B",
                    BuildingKind::Conveyor => "C",
                    _ => "T",
                };
                let unit = language.tr(Tr::Crystals).to_lowercase();
                // « 1 cristal », « 10 cristaux » / « 1 crystal », « 10 crystals ».
                let unit = match (kind.cost(), *language) {
                    (1, Language::French) => "cristal".to_string(),
                    (1, Language::English) => "crystal".to_string(),
                    _ => unit,
                };
                format!(
                    "{} [{}]\n{} {}",
                    language.tr(Tr::Building(kind)),
                    key,
                    kind.cost(),
                    unit
                )
            }
            Tool::Demolish => format!("{} [X]\n50 %", language.tr(Tr::Demolish)),
        };
    }
}

fn update_toast(
    state: Res<BuildState>,
    language: Res<Language>,
    real_time: Res<Time<Real>>,
    mut text: Single<&mut Text, With<ToastText>>,
) {
    let message = match state.error {
        Some((error, until)) if real_time.elapsed_secs() < until => language.tr(Tr::Error(error)),
        _ => "",
    };
    if text.0 != message {
        text.0 = message.to_string();
    }
}

fn update_outcome(
    sim: Res<Sim>,
    language: Res<Language>,
    mut panel: Single<&mut Visibility, With<OutcomePanel>>,
    mut title: Single<(&mut Text, &mut TextColor), With<OutcomeTitle>>,
    mut detail: Single<&mut Text, (With<OutcomeDetail>, Without<OutcomeTitle>)>,
) {
    let Some(outcome) = sim.outcome else {
        **panel = Visibility::Hidden;
        return;
    };
    **panel = Visibility::Inherited;
    let (heading, body, color) = match outcome {
        Outcome::Victory => (Tr::Victory, Tr::VictoryDetail, Color::srgb(0.6, 1.0, 0.85)),
        Outcome::Defeat => (Tr::Defeat, Tr::DefeatDetail, Color::srgb(1.0, 0.35, 0.4)),
    };
    title.0.0 = language.tr(heading).to_string();
    title.1.0 = color;
    detail.0 = language.tr(body).to_string();
}
