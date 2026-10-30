//! Éditeur de programmes des golems : saisie du code, lancement, mode pas à pas,
//! surlignage de la ligne en cours, console et variables.

use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::{
    clipboard::{Clipboard, ClipboardRead},
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
};
use gem_sim::{BuildError, Command, GolemStatus, MAX_GOLEMS, golem::TEMPLATES};

use crate::{
    build::{BlocksWorldInput, BuildState},
    code_buffer::CodeBuffer,
    i18n::{Language, Tr},
    profile::Profile,
    sim::Sim,
};

/// Nombre de lignes de code visibles à la fois.
const VISIBLE_LINES: usize = 13;

const PANEL_COLOR: Color = Color::srgba(0.02, 0.01, 0.05, 0.85);
const CODE_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.02, 0.9);
const CODE_FOCUSED: Color = Color::srgba(0.03, 0.02, 0.08, 0.95);
const BUTTON_COLOR: Color = Color::srgba(0.12, 0.1, 0.18, 0.9);

const CODE_FONT: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono.ttf");
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/Exo2.ttf");

#[derive(Resource, Default)]
pub struct Editor {
    /// Golem dont on édite le programme (éditeur fermé si aucun).
    pub golem: Option<u32>,
    buffer: CodeBuffer,
    /// Le clavier écrit dans le code (les raccourcis du jeu sont alors désactivés).
    pub focused: bool,
    /// Message ponctuel (programme enregistré…), affiché en tête de la console.
    notice: Option<String>,
    scroll: usize,
}

/// Condition d'exécution : vraie quand le joueur tape du code.
pub fn typing(editor: Res<Editor>) -> bool {
    editor.focused
}

#[derive(Resource)]
struct EditorFonts {
    code: Handle<Font>,
    ui: Handle<Font>,
}

#[derive(Component)]
struct EditorPanel;

#[derive(Component)]
struct EditorTitle;

#[derive(Component)]
struct CodeArea;

#[derive(Component)]
struct CodeText;

#[derive(Component)]
struct ConsoleText;

#[derive(Component)]
struct GolemList;

#[derive(Component, Clone, Copy)]
enum EditorButton {
    Run,
    Stop,
    Slow,
    Save,
    Close,
    Template(usize),
    /// Programme de la bibliothèque (usize::MAX : l'étiquette de la rangée).
    Library(usize),
}

/// Rangée des programmes enregistrés dans la bibliothèque.
#[derive(Component)]
struct LibraryRow;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Editor>()
            .add_systems(Startup, spawn_editor)
            .add_systems(
                Update,
                (
                    select_golem_in_world,
                    focus_with_mouse,
                    edit_code.run_if(typing),
                    update_editor_panel,
                    update_code_text,
                    update_console,
                    update_golem_list,
                    update_library_row,
                    update_button_labels,
                )
                    .chain(),
            );
    }
}

fn text_style(font: &Handle<Font>, size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font: font.clone().into(),
            font_size: size.into(),
            ..default()
        },
        TextColor(color),
    )
}

fn button_node() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
        border_radius: BorderRadius::all(Val::Px(5.0)),
        ..default()
    }
}

fn spawn_editor(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let code = fonts.add(Font::from_bytes(CODE_FONT.to_vec()));
    let ui = fonts.add(Font::from_bytes(UI_FONT.to_vec()));
    commands.insert_resource(EditorFonts {
        code: code.clone(),
        ui: ui.clone(),
    });

    commands
        .spawn((
            EditorPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(190.0),
                width: Val::Px(600.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(PANEL_COLOR),
            Visibility::Hidden,
            Hovered::default(),
            BlocksWorldInput,
        ))
        .with_children(|panel| {
            // Titre et commandes.
            panel
                .spawn(Node {
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        EditorTitle,
                        Text::default(),
                        text_style(&ui, 17.0, Color::srgb(0.78, 0.55, 1.0)),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    for button in [
                        EditorButton::Run,
                        EditorButton::Stop,
                        EditorButton::Slow,
                        EditorButton::Save,
                        EditorButton::Close,
                    ] {
                        spawn_button(row, button, &ui);
                    }
                });
            // Modèles.
            panel
                .spawn(Node {
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        EditorButton::Template(usize::MAX),
                        Text::default(),
                        text_style(&ui, 13.0, Color::srgb(0.7, 0.7, 0.8)),
                    ));
                    for i in 0..TEMPLATES.len() {
                        spawn_button(row, EditorButton::Template(i), &ui);
                    }
                });
            // Bibliothèque : les programmes enregistrés (reconstruite quand elle change).
            panel.spawn((
                LibraryRow,
                Node {
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(4.0),
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
            // Code.
            panel
                .spawn((
                    CodeArea,
                    Node {
                        height: Val::Px(VISIBLE_LINES as f32 * 17.5 + 16.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        // Les lignes trop longues sont coupées, pas renvoyées à la ligne.
                        overflow: Overflow::clip(),
                        border_radius: BorderRadius::all(Val::Px(5.0)),
                        ..default()
                    },
                    BackgroundColor(CODE_BACKGROUND),
                    Hovered::default(),
                ))
                .observe(|_: On<PointerClick>, mut editor: ResMut<Editor>| {
                    editor.focused = true;
                })
                .with_children(|area| {
                    area.spawn((
                        CodeText,
                        Text::default(),
                        text_style(&code, 14.0, Color::WHITE),
                        TextLayout::no_wrap(),
                    ));
                });
            // Console : messages, erreurs, variables.
            panel.spawn((
                ConsoleText,
                Text::default(),
                text_style(&code, 13.0, Color::srgb(0.75, 0.9, 0.8)),
            ));
        });

    // Liste des golems, en bas à droite.
    commands.spawn((
        GolemList,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(100.0),
            min_width: Val::Px(200.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(10.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.01, 0.05, 0.6)),
        Hovered::default(),
        BlocksWorldInput,
    ));
}

fn spawn_button(parent: &mut ChildSpawnerCommands, button: EditorButton, font: &Handle<Font>) {
    parent
        .spawn((button_node(), BackgroundColor(BUTTON_COLOR)))
        .observe(
            move |_: On<PointerClick>,
                  mut editor: ResMut<Editor>,
                  mut sim: ResMut<Sim>,
                  mut profile: ResMut<Profile>,
                  language: Res<Language>| {
                let Some(id) = editor.golem else {
                    return;
                };
                match button {
                    EditorButton::Run => {
                        editor.focused = false;
                        let source = editor.buffer.text.clone();
                        sim.queue(Command::RunProgram { golem: id, source });
                    }
                    EditorButton::Stop => sim.queue(Command::StopProgram { golem: id }),
                    EditorButton::Slow => {
                        let slow = sim.golem(id).is_some_and(|g| !g.slow);
                        sim.queue(Command::SetSlow { golem: id, slow });
                    }
                    EditorButton::Close => {
                        editor.golem = None;
                        editor.focused = false;
                    }
                    EditorButton::Template(i) if i < TEMPLATES.len() => {
                        let (_, french, english) = TEMPLATES[i];
                        let text = if *language == Language::French {
                            french
                        } else {
                            english
                        };
                        editor.buffer = CodeBuffer::new(text);
                        editor.scroll = 0;
                    }
                    EditorButton::Template(_) => {}
                    EditorButton::Save => {
                        let name = profile.save_script(&editor.buffer.text);
                        editor.notice = Some(format!("{} {name}", language.tr(Tr::Saved)));
                    }
                    EditorButton::Library(i) => {
                        if let Some(script) = profile.scripts.get(i) {
                            editor.buffer = CodeBuffer::new(script.source.clone());
                            editor.scroll = 0;
                        }
                    }
                }
            },
        )
        .with_children(|b| {
            b.spawn((
                button,
                Text::default(),
                text_style(font, 14.0, Color::WHITE),
            ));
        });
}

/// Ouvre l'éditeur sur un golem, avec son programme (ou un modèle s'il n'en a pas).
fn open(editor: &mut Editor, sim: &Sim, id: u32, language: Language) {
    let Some(golem) = sim.golem(id) else {
        return;
    };
    let source = if golem.source.is_empty() {
        let (_, french, english) = TEMPLATES[0];
        if language == Language::French {
            french
        } else {
            english
        }
        .to_string()
    } else {
        golem.source.clone()
    };
    editor.golem = Some(id);
    editor.buffer = CodeBuffer::new(source);
    editor.scroll = 0;
    editor.focused = false;
}

/// Sans outil de construction, un clic sur un golem ouvre son programme.
fn select_golem_in_world(
    buttons: Res<ButtonInput<MouseButton>>,
    state: Res<BuildState>,
    sim: Res<Sim>,
    language: Res<Language>,
    mut editor: ResMut<Editor>,
) {
    if !buttons.just_pressed(MouseButton::Left) || state.tool.is_some() {
        return;
    }
    let Some(cursor) = state.cursor else {
        return;
    };
    let clicked = sim.golems.iter().find(|g| {
        let tile = g.pos.tile();
        (tile.x - cursor.x).abs() <= 1 && (tile.y - cursor.y).abs() <= 1
    });
    if let Some(golem) = clicked {
        let id = golem.id;
        open(&mut editor, &sim, id, *language);
    }
}

/// Un clic hors du code rend le clavier au jeu.
fn focus_with_mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    area: Single<&Hovered, With<CodeArea>>,
    mut editor: ResMut<Editor>,
) {
    if buttons.just_pressed(MouseButton::Left) && !area.get() && editor.focused {
        editor.focused = false;
    }
    if editor.golem.is_none() && editor.focused {
        editor.focused = false;
    }
}

fn edit_code(
    mut keys: MessageReader<KeyboardInput>,
    pressed: Res<ButtonInput<KeyCode>>,
    mut clipboard: ResMut<Clipboard>,
    mut paste: Local<Option<ClipboardRead>>,
    mut editor: ResMut<Editor>,
) {
    // Collage en attente (asynchrone dans le navigateur).
    if let Some(read) = paste.as_mut()
        && let Some(result) = read.poll_result()
    {
        if let Ok(text) = result {
            editor.buffer.insert(&text);
        }
        *paste = None;
    }

    let command = pressed.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        let buffer = &mut editor.buffer;
        match &key.logical_key {
            Key::Enter => buffer.newline(),
            Key::Backspace => buffer.backspace(),
            Key::Delete => buffer.delete(),
            Key::ArrowLeft => buffer.left(),
            Key::ArrowRight => buffer.right(),
            Key::ArrowUp => buffer.up(),
            Key::ArrowDown => buffer.down(),
            Key::Home => buffer.home(),
            Key::End => buffer.end(),
            Key::Tab => buffer.tab(),
            Key::Escape => editor.focused = false,
            _ if command && key.key_code == KeyCode::KeyV => {
                *paste = Some(clipboard.fetch_text());
            }
            _ if command => {}
            _ => {
                if let Some(text) = &key.text
                    && text.chars().all(|c| !c.is_control())
                {
                    buffer.insert(text);
                }
            }
        }
    }
}

fn status_label(status: &GolemStatus, language: Language) -> String {
    match status {
        GolemStatus::Idle => language.tr(Tr::StatusIdle).into(),
        GolemStatus::Running => language.tr(Tr::StatusRunning).into(),
        GolemStatus::Finished => language.tr(Tr::StatusFinished).into(),
        GolemStatus::Error(error) => {
            format!(
                "{} ({} {})",
                language.tr(Tr::StatusError),
                if language == Language::French {
                    "ligne"
                } else {
                    "line"
                },
                error.line
            )
        }
    }
}

fn update_editor_panel(
    sim: Res<Sim>,
    language: Res<Language>,
    mut editor: ResMut<Editor>,
    mut panel: Single<&mut Visibility, With<EditorPanel>>,
    mut title: Single<&mut Text, With<EditorTitle>>,
    mut area: Single<&mut BackgroundColor, With<CodeArea>>,
) {
    // Le golem a disparu (nouvelle planète) : on ferme.
    if editor.golem.is_some_and(|id| sim.golem(id).is_none()) {
        editor.golem = None;
    }
    let Some(golem) = editor.golem.and_then(|id| sim.golem(id)) else {
        **panel = Visibility::Hidden;
        return;
    };
    **panel = Visibility::Inherited;
    title.0 = format!(
        "{} {} — {}",
        language.tr(Tr::Golem),
        golem.id,
        status_label(&golem.status, *language)
    );
    area.0 = if editor.focused {
        CODE_FOCUSED
    } else {
        CODE_BACKGROUND
    };
}

fn update_button_labels(
    sim: Res<Sim>,
    editor: Res<Editor>,
    language: Res<Language>,
    profile: Res<Profile>,
    mut labels: Query<(&EditorButton, &mut Text)>,
) {
    let slow = editor
        .golem
        .and_then(|id| sim.golem(id))
        .is_some_and(|g| g.slow);
    for (button, mut text) in &mut labels {
        let label = match button {
            EditorButton::Run => language.tr(Tr::Run),
            EditorButton::Stop => language.tr(Tr::Stop),
            EditorButton::Slow if slow => language.tr(Tr::Slow),
            EditorButton::Slow => language.tr(Tr::Fast),
            EditorButton::Close => language.tr(Tr::Close),
            EditorButton::Template(i) if *i < TEMPLATES.len() => TEMPLATES[*i].0,
            EditorButton::Template(_) => language.tr(Tr::Templates),
            EditorButton::Save => language.tr(Tr::SaveScript),
            EditorButton::Library(i) => profile
                .scripts
                .get(*i)
                .map_or(language.tr(Tr::Library), |s| s.name.as_str()),
        };
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
}

/// Le code, avec numéros de ligne, curseur, ligne en cours (jaune) et ligne en erreur (rouge).
#[allow(clippy::too_many_arguments)]
fn update_code_text(
    mut commands: Commands,
    sim: Res<Sim>,
    fonts: Res<EditorFonts>,
    mut editor: ResMut<Editor>,
    code: Single<Entity, With<CodeText>>,
    mut shown: Local<u64>,
) {
    let Some(golem) = editor.golem.and_then(|id| sim.golem(id)) else {
        return;
    };
    let error_line = match &golem.status {
        GolemStatus::Error(error) => Some(error.line as usize),
        _ => None,
    };
    let running_line = (golem.status == GolemStatus::Running)
        .then_some(golem.line)
        .flatten()
        .map(|l| l as usize);
    let (cursor_line, cursor_col) = editor.buffer.line_col();

    // Faire défiler pour garder visible le curseur (en écriture) ou la ligne exécutée.
    let follow = if editor.focused {
        Some(cursor_line)
    } else {
        running_line.or(error_line).map(|l| l.saturating_sub(1))
    };
    if let Some(line) = follow {
        if line < editor.scroll {
            editor.scroll = line;
        } else if line >= editor.scroll + VISIBLE_LINES {
            editor.scroll = line + 1 - VISIBLE_LINES;
        }
    }

    let mut hasher = DefaultHasher::new();
    (
        &editor.buffer,
        editor.focused,
        editor.scroll,
        running_line,
        error_line,
    )
        .hash(&mut hasher);
    let signature = hasher.finish();
    if *shown == signature {
        return;
    }
    *shown = signature;

    let font = |size: f32| TextFont {
        font: fonts.code.clone().into(),
        font_size: size.into(),
        ..default()
    };
    let lines: Vec<&str> = editor.buffer.text.split('\n').collect();
    let mut spans: Vec<(String, Color)> = Vec::new();
    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(editor.scroll)
        .take(VISIBLE_LINES)
    {
        let number = index + 1;
        let (marker, number_color, text_color) = if Some(number) == error_line {
            (
                "!",
                Color::srgb(1.0, 0.35, 0.35),
                Color::srgb(1.0, 0.6, 0.6),
            )
        } else if Some(number) == running_line {
            (
                "▸",
                Color::srgb(1.0, 0.85, 0.3),
                Color::srgb(1.0, 0.95, 0.7),
            )
        } else {
            (
                " ",
                Color::srgb(0.4, 0.42, 0.5),
                Color::srgb(0.85, 0.88, 0.95),
            )
        };
        spans.push((format!("{marker}{number:>3} "), number_color));
        if editor.focused && index == cursor_line {
            let split = line
                .char_indices()
                .nth(cursor_col)
                .map_or(line.len(), |(i, _)| i);
            spans.push((line[..split].to_string(), text_color));
            spans.push(("│".to_string(), Color::srgb(0.5, 0.9, 1.0)));
            spans.push((line[split..].to_string(), text_color));
        } else {
            spans.push((line.to_string(), text_color));
        }
        spans.push(("\n".to_string(), text_color));
    }

    commands.entity(*code).despawn_children();
    commands.entity(*code).with_children(|parent| {
        for (text, color) in spans {
            parent.spawn((TextSpan::new(text), font(14.0), TextColor(color)));
        }
    });
}

fn update_console(
    sim: Res<Sim>,
    editor: Res<Editor>,
    language: Res<Language>,
    mut console: Single<(&mut Text, &mut TextColor), With<ConsoleText>>,
) {
    let Some(golem) = editor.golem.and_then(|id| sim.golem(id)) else {
        return;
    };
    let mut lines = vec![language.tr(Tr::EditorHint).to_string()];
    if let Some(notice) = &editor.notice {
        lines.push(notice.clone());
    }
    if editor.focused {
        let (line, col) = editor.buffer.line_col();
        lines[0] = format!("{} {}:{}", lines[0], line + 1, col + 1);
    }
    let error = matches!(golem.status, GolemStatus::Error(_));
    match &golem.status {
        GolemStatus::Error(error) => lines.push(error.message(language.script())),
        _ => {
            let variables: Vec<String> = golem
                .variables()
                .into_iter()
                .map(|(name, value)| {
                    let value: String = value.chars().take(30).collect();
                    format!("{name} = {value}")
                })
                .collect();
            if !variables.is_empty() {
                lines.push(variables.join("   "));
            }
        }
    }
    lines.push(format!("— {} —", language.tr(Tr::Console)));
    lines.extend(golem.log.iter().rev().take(3).rev().cloned());
    let text = lines.join("\n");
    if console.0.0 != text {
        console.0.0 = text;
    }
    console.1.0 = if error {
        Color::srgb(1.0, 0.6, 0.55)
    } else {
        Color::srgb(0.75, 0.9, 0.8)
    };
}

/// Golems affichés (identifiant, état) et langue, pour ne reconstruire la liste qu'au besoin.
type ShownGolems = (Vec<(u32, String)>, Language, u32);

/// Liste des golems (cliquer pour ouvrir son programme) et bouton pour en fabriquer.
fn update_golem_list(
    mut commands: Commands,
    sim: Res<Sim>,
    language: Res<Language>,
    fonts: Res<EditorFonts>,
    list: Single<Entity, With<GolemList>>,
    mut shown: Local<Option<ShownGolems>>,
) {
    let rows: Vec<(u32, String)> = sim
        .golems
        .iter()
        .map(|g| (g.id, status_label(&g.status, *language)))
        .collect();
    let current = (rows.clone(), *language, sim.config.bonuses.golem_cost);
    if shown.as_ref() == Some(&current) {
        return;
    }
    *shown = Some(current);
    let lang = *language;
    let ui = fonts.ui.clone();

    commands.entity(*list).despawn_children();
    commands.entity(*list).with_children(|parent| {
        parent.spawn((
            Text::new(lang.tr(Tr::Golems)),
            text_style(&ui, 17.0, Color::srgb(0.78, 0.55, 1.0)),
        ));
        if rows.is_empty() {
            parent.spawn((
                Text::new(lang.tr(Tr::NoGolems)),
                text_style(&ui, 13.0, Color::srgb(0.7, 0.7, 0.78)),
            ));
        }
        for (id, status) in rows {
            parent
                .spawn((button_node(), BackgroundColor(BUTTON_COLOR)))
                .observe(
                    move |_: On<PointerClick>,
                          mut editor: ResMut<Editor>,
                          sim: Res<Sim>,
                          language: Res<Language>| {
                        open(&mut editor, &sim, id, *language);
                    },
                )
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!("{} {id} · {status}", lang.tr(Tr::Golem))),
                        text_style(&ui, 14.0, Color::WHITE),
                    ));
                });
        }
        if sim.golems.len() < MAX_GOLEMS {
            parent
                .spawn((
                    button_node(),
                    BackgroundColor(Color::srgba(0.3, 0.15, 0.5, 0.9)),
                ))
                .observe(
                    |_: On<PointerClick>,
                     mut sim: ResMut<Sim>,
                     mut state: ResMut<BuildState>,
                     time: Res<Time<Real>>| {
                        if sim.stock >= sim.config.bonuses.golem_cost {
                            sim.queue(Command::SpawnGolem);
                        } else {
                            state.error = Some((
                                Tr::Error(BuildError::NotEnoughCrystals),
                                time.elapsed_secs() + 2.5,
                            ));
                        }
                    },
                )
                .with_children(|row| {
                    row.spawn((
                        Text::new(format!(
                            "{} ({} {})",
                            lang.tr(Tr::NewGolem),
                            sim.config.bonuses.golem_cost,
                            lang.tr(Tr::Crystals).to_lowercase()
                        )),
                        text_style(&ui, 14.0, Color::WHITE),
                    ));
                });
        }
    });
}

/// Reconstruit la rangée « Bibliothèque » quand la liste des programmes change.
fn update_library_row(
    mut commands: Commands,
    profile: Res<Profile>,
    fonts: Res<EditorFonts>,
    row: Single<Entity, With<LibraryRow>>,
    mut shown: Local<Option<Vec<String>>>,
) {
    let names: Vec<String> = profile.scripts.iter().map(|s| s.name.clone()).collect();
    if shown.as_ref() == Some(&names) {
        return;
    }
    *shown = Some(names.clone());
    let font = fonts.ui.clone();
    commands.entity(*row).despawn_children();
    if names.is_empty() {
        return;
    }
    commands.entity(*row).with_children(|row| {
        row.spawn((
            EditorButton::Library(usize::MAX),
            Text::default(),
            text_style(&font, 13.0, Color::srgb(0.7, 0.7, 0.8)),
        ));
        for i in 0..names.len() {
            spawn_button(row, EditorButton::Library(i), &font);
        }
    });
}
