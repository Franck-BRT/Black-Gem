//! Barre de commandes tactiles : tout ce qui passait par le clavier (pause, vitesse, tourner,
//! annuler, aller au QG ou au Cœur) devient un bouton. Affichée sur écran tactile (ou si les
//! réglages le demandent).

use bevy::{
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};

use crate::{
    build::{BlocksWorldInput, BuildState, cancel_tool},
    camera::RtsCamera,
    i18n::Language,
    mission::MissionMenu,
    settings::Effective,
    sim::{GameSpeed, SPEEDS, Sim},
    world,
};

#[derive(Component)]
struct TouchBar;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum TouchButton {
    Pause,
    Speed,
    Rotate,
    Cancel,
    Hq,
    Core,
}

pub struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_bar)
            .add_systems(Update, update_bar);
    }
}

fn spawn_bar(mut commands: Commands) {
    commands
        .spawn((
            TouchBar,
            crate::blocks::HideWhileCoding,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(96.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(10.0),
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            for button in [
                TouchButton::Pause,
                TouchButton::Speed,
                TouchButton::Rotate,
                TouchButton::Cancel,
                TouchButton::Hq,
                TouchButton::Core,
            ] {
                bar.spawn((
                    button,
                    Node {
                        min_width: Val::Px(84.0),
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.1, 0.2, 0.88)),
                    Hovered::default(),
                    BlocksWorldInput,
                ))
                .observe(press)
                .with_children(|b| {
                    b.spawn((
                        Text::default(),
                        TextFont {
                            font_size: 17.0.into(),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));
                });
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn press(
    click: On<PointerClick>,
    buttons: Query<&TouchButton>,
    sim: Res<Sim>,
    mut speed: ResMut<GameSpeed>,
    mut build: ResMut<BuildState>,
    mut camera: Single<&mut RtsCamera>,
) {
    let Ok(button) = buttons.get(click.entity) else {
        return;
    };
    match button {
        TouchButton::Pause => speed.paused = !speed.paused,
        TouchButton::Speed => {
            speed.index = (speed.index + 1) % SPEEDS.len();
            speed.paused = false;
        }
        TouchButton::Rotate => build.dir = build.dir.clockwise(),
        TouchButton::Cancel => cancel_tool(&mut build),
        TouchButton::Hq => camera.look_at(world::hq_center(&sim)),
        TouchButton::Core => {
            if let Some(core) = sim.core {
                let center =
                    world::tile_to_world(&sim.map, gem_sim::Tile::new(core.x + 1, core.y + 1));
                camera.look_at(center.xz());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_bar(
    effective: Res<Effective>,
    menu: Res<MissionMenu>,
    language: Res<Language>,
    speed: Res<GameSpeed>,
    build: Res<BuildState>,
    sim: Res<Sim>,
    mut bar: Single<&mut Node, With<TouchBar>>,
    buttons: Query<(&TouchButton, &Children, &mut Node), Without<TouchBar>>,
    mut texts: Query<&mut Text>,
) {
    let visible = effective.touch_ui && !menu.open;
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if bar.display != display {
        bar.display = display;
    }
    if !visible {
        return;
    }
    let french = *language == Language::French;
    for (button, children, mut node) in buttons {
        let label = match button {
            TouchButton::Pause if speed.paused => {
                if french { "Reprendre" } else { "Resume" }.to_string()
            }
            TouchButton::Pause => "Pause".to_string(),
            TouchButton::Speed => format!("×{}", SPEEDS[speed.index] as u32),
            TouchButton::Rotate => if french { "Tourner" } else { "Rotate" }.to_string(),
            TouchButton::Cancel => if french { "Annuler" } else { "Cancel" }.to_string(),
            TouchButton::Hq => if french { "QG" } else { "HQ" }.to_string(),
            TouchButton::Core => if french { "Cœur" } else { "Core" }.to_string(),
        };
        // Tourner et Annuler n'ont de sens qu'avec un outil ; le Cœur, s'il existe.
        let shown = match button {
            TouchButton::Rotate | TouchButton::Cancel => build.tool.is_some(),
            TouchButton::Core => sim.core.is_some(),
            _ => true,
        };
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if let Some(&child) = children.first()
            && let Ok(mut text) = texts.get_mut(child)
            && text.0 != label
        {
            text.0 = label;
        }
    }
}
