//! Tutoriel de la première expédition : une liste d'étapes qui se cochent toutes seules en
//! regardant l'état de la partie, avec le bouton utile qui clignote dans la barre d'outils.
//!
//! Il s'affiche tant que le profil ne l'a pas marqué comme terminé ; F1 le réaffiche.

use bevy::{
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
    ui::Outline,
};
use gem_sim::{BuildingKind, BuildingState, Format, GolemStatus, Simulation};

use crate::{
    build::{BlocksWorldInput, Tool},
    editor::{Editor, typing},
    hud::ToolButton,
    i18n::Language,
    mission::MissionMenu,
    profile::Profile,
    sim::Sim,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Drill,
    Deliver,
    Tower,
    Feed,
    Golem,
    Night,
    Core,
}

impl Step {
    const ALL: [Step; 7] = [
        Step::Drill,
        Step::Deliver,
        Step::Tower,
        Step::Feed,
        Step::Golem,
        Step::Night,
        Step::Core,
    ];

    fn done(self, sim: &Simulation) -> bool {
        let has = |kind| sim.buildings.iter().any(|b| b.kind == kind);
        match self {
            Step::Drill => has(BuildingKind::Drill),
            Step::Deliver => sim.delivered > 0,
            Step::Tower => has(BuildingKind::Tower),
            Step::Feed => sim.buildings.iter().any(|b| {
                matches!(b.state, BuildingState::Tower { crystals, charge, .. } if crystals + charge > 0)
            }),
            Step::Golem => sim.golems.iter().any(|g| g.status == GolemStatus::Running),
            Step::Night => sim.wave >= 1 && !sim.is_night(),
            Step::Core => sim.core.is_none() || has(BuildingKind::CoreDrill),
        }
    }

    /// Le bouton de la barre d'outils à mettre en évidence.
    fn tool(self) -> Option<Tool> {
        match self {
            Step::Drill => Some(Tool::Build(BuildingKind::Drill)),
            Step::Deliver | Step::Feed => Some(Tool::Build(BuildingKind::Conveyor)),
            Step::Tower => Some(Tool::Build(BuildingKind::Tower)),
            Step::Core => Some(Tool::Build(BuildingKind::CoreDrill)),
            Step::Golem | Step::Night => None,
        }
    }

    fn text(self, french: bool) -> &'static str {
        match (self, french) {
            (Step::Drill, true) => {
                "Construis une foreuse [B] sur un filon : les cristaux lumineux. Clic pour la poser, R pour la tourner."
            }
            (Step::Drill, false) => {
                "Build a drill [B] on a vein: the glowing crystals. Click to place it, R to rotate it."
            }
            (Step::Deliver, true) => {
                "Relie sa sortie au QG avec des convoyeurs [C] : clic et glisse pour tracer la ligne. Les chevrons montrent le sens."
            }
            (Step::Deliver, false) => {
                "Link its output to the HQ with conveyors [C]: click and drag to draw the line. The chevrons show the direction."
            }
            (Step::Tower, true) => {
                "La nuit, la Corruption attaque. Construis une tour [T] près du QG."
            }
            (Step::Tower, false) => {
                "At night, the Corruption attacks. Build a tower [T] near the HQ."
            }
            (Step::Feed, true) => {
                "Une tour tire avec des cristaux : amène-lui un convoyeur depuis une foreuse (ou un golem plus tard)."
            }
            (Step::Feed, false) => {
                "A tower fires crystals: bring it a conveyor from a drill (or a golem later)."
            }
            (Step::Golem, true) => {
                "Fabrique un golem (en bas à droite), clique dessus, choisis le modèle Ravitailleur et lance-le."
            }
            (Step::Golem, false) => {
                "Build a golem (bottom right), click it, pick the Supplier template and run it."
            }
            (Step::Night, true) => {
                "Tiens jusqu'au matin. Les ennemis détruisent ce qui leur barre la route : protège tes lignes."
            }
            (Step::Night, false) => {
                "Hold until morning. Enemies destroy whatever blocks their way: protect your lines."
            }
            (Step::Core, true) => {
                "L'objectif : construis le Grand foreur [O] sur le Cœur (touche V pour y aller) et alimente-le en cristaux."
            }
            (Step::Core, false) => {
                "The goal: build the Great drill [O] on the Core (press V to go there) and feed it crystals."
            }
        }
    }
}

#[derive(Resource, Default)]
struct Tutorial {
    active: bool,
    /// Moment où toutes les étapes ont été faites (le message de fin reste affiché un peu).
    finished_at: Option<f32>,
}

#[derive(Component)]
struct TutorialPanel;

#[derive(Component)]
struct TutorialText;

#[derive(Component)]
struct SkipLabel;

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tutorial>()
            .add_systems(Startup, spawn_panel)
            .add_systems(
                Update,
                (toggle.run_if(not(typing)), update_tutorial, highlight_tool).chain(),
            );
    }
}

fn spawn_panel(mut commands: Commands, profile: Res<Profile>, mut tutorial: ResMut<Tutorial>) {
    tutorial.active = !profile.tutorial_done;
    commands
        .spawn((
            TutorialPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(205.0),
                width: Val::Px(380.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::left(Val::Px(3.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.03, 0.1, 0.85)),
            BorderColor::all(Color::srgb(0.75, 0.5, 1.0)),
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|panel| {
            panel.spawn((
                TutorialText,
                Text::default(),
                TextFont {
                    font_size: 15.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(0.92, 0.9, 0.98)),
            ));
            panel
                .spawn((
                    Node {
                        align_self: AlignSelf::FlexEnd,
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border_radius: BorderRadius::all(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.2, 0.15, 0.3, 0.9)),
                ))
                .observe(
                    |_: On<PointerClick>,
                     mut tutorial: ResMut<Tutorial>,
                     mut profile: ResMut<Profile>| {
                        tutorial.active = false;
                        profile.tutorial_done = true;
                    },
                )
                .with_children(|button| {
                    button.spawn((
                        SkipLabel,
                        Text::default(),
                        TextFont {
                            font_size: 13.0.into(),
                            ..default()
                        },
                    ));
                });
        });
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, mut tutorial: ResMut<Tutorial>) {
    if keys.just_pressed(KeyCode::F1) {
        tutorial.active = !tutorial.active;
        tutorial.finished_at = None;
    }
}

/// Première étape pas encore faite (aucune : tout est fait).
fn current_step(sim: &Simulation) -> Option<(usize, Step)> {
    Step::ALL
        .into_iter()
        .enumerate()
        .find(|(_, s)| !s.done(sim))
}

#[allow(clippy::too_many_arguments)]
fn update_tutorial(
    sim: Res<Sim>,
    menu: Res<MissionMenu>,
    editor: Res<Editor>,
    language: Res<Language>,
    time: Res<Time<Real>>,
    mut tutorial: ResMut<Tutorial>,
    mut profile: ResMut<Profile>,
    mut panel: Single<&mut Node, With<TutorialPanel>>,
    mut text: Single<&mut Text, (With<TutorialText>, Without<SkipLabel>)>,
    mut skip: Single<&mut Text, (With<SkipLabel>, Without<TutorialText>)>,
) {
    // « Revoir le tutoriel » (réglages) : il repart.
    if profile.is_changed() && !profile.tutorial_done && !tutorial.active {
        tutorial.active = true;
        tutorial.finished_at = None;
    }
    let french = *language == Language::French;
    let colony = sim.config.format == Format::Colony;
    let visible = tutorial.active && !menu.open && editor.golem.is_none();
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if panel.display != display {
        panel.display = display;
    }
    if !tutorial.active {
        return;
    }
    let content = match current_step(&sim) {
        Some((index, step)) => {
            tutorial.finished_at = None;
            let total = if colony {
                Step::ALL.len() - 1
            } else {
                Step::ALL.len()
            };
            let header = if french {
                format!("Tutoriel — étape {} / {total}", index + 1)
            } else {
                format!("Tutorial — step {} / {total}", index + 1)
            };
            format!("{header}\n{}", step.text(french))
        }
        None => {
            let now = time.elapsed_secs();
            let finished = *tutorial.finished_at.get_or_insert(now);
            if !profile.tutorial_done {
                profile.tutorial_done = true;
            }
            if now - finished > 10.0 {
                tutorial.active = false;
            }
            if french {
                "Bravo ! Tu connais l'essentiel. Le reste : les gemmes (taillerie, four), les golems programmés, et l'Arche entre deux expéditions. F1 : revoir ce guide.".to_string()
            } else {
                "Well done! You know the basics. Next: gems (cutter, furnace), programmed golems, and the Ark between expeditions. F1: show this guide again.".to_string()
            }
        }
    };
    if text.0 != content {
        text.0 = content;
    }
    let label = if french {
        "Passer le tutoriel"
    } else {
        "Skip the tutorial"
    };
    if skip.0 != label {
        skip.0 = label.to_string();
    }
}

/// Le bouton de l'étape en cours clignote.
fn highlight_tool(
    mut commands: Commands,
    sim: Res<Sim>,
    tutorial: Res<Tutorial>,
    menu: Res<MissionMenu>,
    time: Res<Time<Real>>,
    buttons: Query<(Entity, &ToolButton, Option<&Outline>)>,
) {
    let wanted = (tutorial.active && !menu.open)
        .then(|| current_step(&sim))
        .flatten()
        .and_then(|(_, step)| step.tool());
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 5.0).sin();
    for (entity, button, outline) in &buttons {
        if Some(button.0) == wanted {
            commands.entity(entity).insert(Outline {
                width: Val::Px(2.0),
                offset: Val::Px(2.0),
                color: Color::srgba(1.0, 0.85, 0.3, 0.3 + 0.7 * pulse),
            });
        } else if outline.is_some() {
            commands.entity(entity).remove::<Outline>();
        }
    }
}
