//! Interface minimale : titre, heure de la planète, vitesse, aide des commandes.

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use gem_sim::TICKS_PER_DAY;

use crate::{
    i18n::{Language, Tr},
    sim::{GameSpeed, SPEEDS, Sim},
};

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct HelpText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (update_status, update_help));
    }
}

/// Police de l'interface (Exo 2, licence OFL) : intégrée au binaire, avec tous les accents.
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/Exo2.ttf");

fn spawn_hud(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = fonts.add(Font::from_bytes(UI_FONT.to_vec()));
    let panel = Node {
        position_type: PositionType::Absolute,
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.0),
        padding: UiRect::all(Val::Px(10.0)),
        ..default()
    };
    let shadow = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45));

    commands
        .spawn((
            Node {
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                ..panel.clone()
            },
            shadow,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("BLACK GEM"),
                TextFont {
                    font: font.clone().into(),
                    font_size: 28.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.55, 1.0)),
            ));
            parent.spawn((
                StatusText,
                Text::default(),
                TextFont {
                    font: font.clone().into(),
                    font_size: 15.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.92, 0.95)),
            ));
        });

    commands.spawn((
        HelpText,
        Text::default(),
        TextFont {
            font: font.clone().into(),
            font_size: 13.0.into(),
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.87, 0.9)),
        Node {
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..panel
        },
        shadow,
    ));
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

    text.0 = format!(
        "{}\n{} {} · {:02}:{:02} ({})\n{} : {} · {} : {}   {:.0} FPS",
        lang.tr(Tr::Subtitle),
        lang.tr(Tr::Day),
        day,
        minutes_of_day / 60,
        minutes_of_day % 60,
        period,
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
