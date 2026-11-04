//! Éditeur par blocs : pour programmer au doigt (iPhone, tablette), sans clavier.
//!
//! Une palette d'instructions s'affiche à côté du code : toucher une instruction l'ajoute
//! sous la ligne du curseur, à la bonne indentation (un bloc `while …:` ou `loop { … }` ouvre
//! une nouvelle ligne décalée). Des boutons décalent, déplacent ou suppriment la ligne, et
//! changent son nombre (`< 12` → `< 13`). Toucher une ligne du code y place le curseur.
//! Le programme reste du vrai Python ou du vrai Rust, visible et modifiable au clavier.

use bevy::{
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_script::looks_like_rust;

use crate::{build::BlocksWorldInput, editor::Editor, i18n::Language, settings::Effective};

/// Une instruction de la palette : étiquette (fr, en), code (fr, en), ouvre-t-elle un bloc ?
type Snippet = (&'static str, &'static str, &'static str, &'static str, bool);

const PYTHON: [Snippet; 16] = [
    (
        "répéter toujours",
        "repeat forever",
        "while True:",
        "while True:",
        true,
    ),
    (
        "pour chaque tour",
        "for each tower",
        "for x, y in tours():",
        "for x, y in towers():",
        true,
    ),
    (
        "pour chaque foreuse",
        "for each drill",
        "for x, y in foreuses():",
        "for x, y in drills():",
        true,
    ),
    (
        "si munitions < 12",
        "if ammo < 12",
        "if munitions(x, y) < 12:",
        "if ammo(x, y) < 12:",
        true,
    ),
    (
        "si prendre réussit",
        "if take works",
        "if prendre():",
        "if take():",
        true,
    ),
    ("sinon", "else", "else:", "else:", true),
    (
        "aller au QG",
        "go to the HQ",
        "aller_vers(qg())",
        "move_to(hq())",
        false,
    ),
    (
        "aller en x, y",
        "go to x, y",
        "aller_vers(x, y)",
        "move_to(x, y)",
        false,
    ),
    (
        "aller au Cœur",
        "go to the Core",
        "aller_vers(coeur())",
        "move_to(core())",
        false,
    ),
    ("prendre", "take", "prendre()", "take()", false),
    (
        "prendre une gemme",
        "take a gem",
        "prendre_gemme()",
        "take_gem()",
        false,
    ),
    ("déposer", "drop", "deposer()", "drop()", false),
    ("attendre 1 s", "wait 1 s", "attendre(1)", "wait(1)", false),
    (
        "dire",
        "say",
        "dire(\"Bonjour\", x, y)",
        "say(\"Hello\", x, y)",
        false,
    ),
    (
        "si je réserve",
        "if I reserve",
        "if reserver(x, y):",
        "if reserve(x, y):",
        true,
    ),
    (
        "libérer",
        "release",
        "liberer(x, y)",
        "release(x, y)",
        false,
    ),
];

const RUST: [Snippet; 15] = [
    ("fn main", "fn main", "fn main() {", "fn main() {", true),
    (
        "répéter toujours",
        "repeat forever",
        "loop {",
        "loop {",
        true,
    ),
    (
        "pour chaque tour",
        "for each tower",
        "for tour in tours() {",
        "for tower in towers() {",
        true,
    ),
    (
        "pour chaque foreuse",
        "for each drill",
        "for foreuse in foreuses() {",
        "for drill in drills() {",
        true,
    ),
    (
        "si munitions < 12",
        "if ammo < 12",
        "if munitions(tour) < 12 {",
        "if ammo(tower) < 12 {",
        true,
    ),
    (
        "si je prends un objet",
        "if I take an item",
        "if let Some(objet) = prendre() {",
        "if let Some(item) = take() {",
        true,
    ),
    (
        "aller au QG",
        "go to the HQ",
        "aller_vers(qg());",
        "move_to(hq());",
        false,
    ),
    (
        "aller à la tour",
        "go to the tower",
        "aller_vers(tour);",
        "move_to(tower);",
        false,
    ),
    (
        "aller à la foreuse",
        "go to the drill",
        "aller_vers(foreuse);",
        "move_to(drill);",
        false,
    ),
    (
        "déposer l'objet",
        "drop the item",
        "let _ = deposer(objet);",
        "let _ = drop(item);",
        false,
    ),
    (
        "attendre 1 s",
        "wait 1 s",
        "attendre(1);",
        "wait(1);",
        false,
    ),
    (
        "afficher",
        "print",
        "println!(\"{:?}\", tour);",
        "println!(\"{:?}\", tower);",
        false,
    ),
    (
        "si je réserve",
        "if I reserve",
        "if reserver(tour) {",
        "if reserve(tower) {",
        true,
    ),
    (
        "libérer",
        "release",
        "liberer(tour);",
        "release(tower);",
        false,
    ),
    ("ligne vide", "empty line", "", "", false),
];

#[derive(Clone, Copy)]
enum Action {
    Insert(usize, bool),
    Shift(bool),
    Move(bool),
    Cursor(bool),
    Delete,
    Bump(i64),
}

#[derive(Component)]
struct Palette;

/// Interface cachée pendant l'édition d'un programme au doigt (l'éditeur prend l'écran).
#[derive(Component)]
pub struct HideWhileCoding;

#[derive(Component)]
struct PaletteContent;

pub struct BlocksPlugin;

impl Plugin for BlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_palette)
            .add_systems(Update, (default_mode, update_palette, focus_layout).chain());
    }
}

fn spawn_palette(mut commands: Commands) {
    commands
        .spawn((
            Palette,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(622.0),
                top: Val::Px(190.0),
                width: Val::Px(270.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.02, 0.07, 0.88)),
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|panel| {
            panel.spawn((
                PaletteContent,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
            ));
        });
}

/// À l'ouverture de l'éditeur sur un écran tactile, les blocs sont proposés d'office.
fn default_mode(
    effective: Res<Effective>,
    mut editor: ResMut<Editor>,
    mut previous: Local<Option<u32>>,
) {
    if editor.golem != *previous {
        if editor.golem.is_some() && effective.touch_ui {
            editor.blocks = true;
        }
        *previous = editor.golem;
    }
}

fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action, accent: bool) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(if accent {
                Color::srgba(0.3, 0.16, 0.5, 0.95)
            } else {
                Color::srgba(0.12, 0.1, 0.18, 0.95)
            }),
        ))
        .observe(
            move |_: On<PointerClick>, mut editor: ResMut<Editor>, language: Res<Language>| {
                let french = *language == Language::French;
                let buffer = &mut editor.buffer;
                match action {
                    Action::Insert(i, rust_list) => {
                        let (_, _, fr, en, opens) = if rust_list { RUST[i] } else { PYTHON[i] };
                        let code = if french { fr } else { en };
                        let closing = (rust_list && opens).then_some("}");
                        buffer.insert_statement(code, opens, closing);
                    }
                    Action::Shift(right) => buffer.shift_line(right),
                    Action::Move(up) => buffer.move_line(up),
                    Action::Cursor(up) => {
                        if up {
                            buffer.up();
                        } else {
                            buffer.down();
                        }
                        buffer.end();
                    }
                    Action::Delete => buffer.delete_line(),
                    Action::Bump(delta) => buffer.bump_number(delta),
                }
            },
        )
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 13.0.into(),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        });
}

fn row(parent: &mut ChildSpawnerCommands, build: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            column_gap: Val::Px(5.0),
            row_gap: Val::Px(5.0),
            flex_wrap: FlexWrap::Wrap,
            ..default()
        })
        .with_children(build);
}

fn update_palette(
    mut commands: Commands,
    editor: Res<Editor>,
    language: Res<Language>,
    mut palette: Single<&mut Node, With<Palette>>,
    content: Single<Entity, With<PaletteContent>>,
    mut shown: Local<Option<(bool, Language)>>,
) {
    let visible = editor.golem.is_some() && editor.blocks;
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if palette.display != display {
        palette.display = display;
    }
    if !visible {
        return;
    }
    let rust = looks_like_rust(&editor.buffer.text);
    let key = (rust, *language);
    if *shown == Some(key) {
        return;
    }
    *shown = Some(key);
    let french = *language == Language::French;
    let t = |fr: &'static str, en: &'static str| if french { fr } else { en };
    commands.entity(*content).despawn_children();
    commands.entity(*content).with_children(|panel| {
        panel.spawn((
            Text::new(if rust {
                t(
                    "Instructions Rust (touche pour ajouter)",
                    "Rust statements (tap to add)",
                )
            } else {
                t(
                    "Instructions Python (touche pour ajouter)",
                    "Python statements (tap to add)",
                )
            }),
            TextFont {
                font_size: 13.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.78, 0.55, 1.0)),
        ));
        row(panel, |r| {
            let list: &[Snippet] = if rust { &RUST } else { &PYTHON };
            for (i, (fr, en, _, _, opens)) in list.iter().enumerate() {
                button(r, t(fr, en), Action::Insert(i, rust), *opens);
            }
        });
        panel.spawn((
            Text::new(t("Ligne du curseur", "Cursor line")),
            TextFont {
                font_size: 13.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.78, 0.55, 1.0)),
        ));
        row(panel, |r| {
            button(r, t("Précédente", "Previous"), Action::Cursor(true), false);
            button(r, t("Suivante", "Next"), Action::Cursor(false), false);
            button(r, t("Décaler", "Indent"), Action::Shift(true), false);
            button(r, t("Reculer", "Unindent"), Action::Shift(false), false);
            button(r, t("Monter", "Move up"), Action::Move(true), false);
            button(r, t("Descendre", "Move down"), Action::Move(false), false);
            button(r, "−1", Action::Bump(-1), false);
            button(r, "+1", Action::Bump(1), false);
            button(r, t("Supprimer", "Delete"), Action::Delete, false);
        });
    });
}

/// Au doigt, pendant l'édition : seuls l'éditeur et la palette restent à l'écran.
fn focus_layout(
    editor: Res<Editor>,
    effective: Res<Effective>,
    mut hidden: Query<&mut Visibility, With<HideWhileCoding>>,
    mut palette: Single<&mut Node, With<Palette>>,
) {
    let focus = effective.touch_ui && editor.golem.is_some();
    let wanted = if focus {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut visibility in &mut hidden {
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let top = if effective.touch_ui { 12.0 } else { 190.0 };
    if palette.top != Val::Px(top) {
        palette.top = Val::Px(top);
    }
}
