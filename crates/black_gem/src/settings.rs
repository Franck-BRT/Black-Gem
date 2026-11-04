//! Réglages : qualité graphique, taille de l'interface, commandes tactiles, son, langue.
//!
//! « Auto » choisit selon l'appareil : qualité basse et interface plus grande sur un écran
//! tactile (iPhone, tablette), moyenne dans un navigateur d'ordinateur, haute sinon.

use bevy::{
    anti_alias::fxaa::Fxaa, input::touch::TouchInput, light::DirectionalLightShadowMap,
    picking::events::PointerClick, post_process::bloom::Bloom, prelude::*, render::view::Msaa,
    window::PrimaryWindow,
};
use serde::{Deserialize, Serialize};

use crate::{
    camera::RtsCamera, i18n::Language, mission::SettingsPage, particles::ParticleBudget,
    profile::Profile,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    #[default]
    Auto,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiSize {
    #[default]
    Auto,
    Small,
    Normal,
    Large,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchControls {
    #[default]
    Auto,
    On,
    Off,
}

/// Ce que l'on sait de l'appareil.
#[derive(Resource, Default)]
pub struct Device {
    /// Un écran tactile a été détecté (ou annoncé par le navigateur).
    pub touch: bool,
}

/// Réglages réellement appliqués (« Auto » résolu).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effective {
    pub quality: Quality,
    /// Afficher les boutons tactiles et l'éditeur par blocs.
    pub touch_ui: bool,
}

impl Default for Effective {
    fn default() -> Self {
        Self {
            quality: Quality::High,
            touch_ui: false,
        }
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Device>()
            .init_resource::<Effective>()
            .add_systems(Startup, detect_web_touch)
            .add_systems(
                Update,
                (
                    detect_touch,
                    resolve,
                    apply_quality,
                    apply_ui_scale,
                    update_page,
                )
                    .chain(),
            );
    }
}

#[cfg(target_arch = "wasm32")]
fn detect_web_touch(mut device: ResMut<Device>) {
    if let Some(window) = web_sys::window() {
        device.touch = window.navigator().max_touch_points() > 0;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_web_touch() {}

fn detect_touch(mut touches: MessageReader<TouchInput>, mut device: ResMut<Device>) {
    if touches.read().next().is_some() && !device.touch {
        device.touch = true;
    }
}

fn resolve(profile: Res<Profile>, device: Res<Device>, mut effective: ResMut<Effective>) {
    let web = cfg!(target_arch = "wasm32");
    let quality = match profile.quality {
        Quality::Auto if device.touch => Quality::Low,
        Quality::Auto if web => Quality::Medium,
        Quality::Auto => Quality::High,
        chosen => chosen,
    };
    let touch_ui = match profile.touch_controls {
        TouchControls::Auto => device.touch,
        TouchControls::On => true,
        TouchControls::Off => false,
    };
    let next = Effective { quality, touch_ui };
    if *effective != next {
        *effective = next;
    }
}

/// Ombres, bloom, occlusion ambiante, anticrénelage et nombre de particules.
fn apply_quality(
    mut commands: Commands,
    effective: Res<Effective>,
    camera: Single<Entity, With<RtsCamera>>,
    mut lights: Query<&mut DirectionalLight>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut budget: ResMut<ParticleBudget>,
    mut applied: Local<Option<Quality>>,
) {
    if *applied == Some(effective.quality) {
        return;
    }
    *applied = Some(effective.quality);
    let quality = effective.quality;
    for mut light in &mut lights {
        light.shadow_maps_enabled = quality != Quality::Low;
    }
    shadow_map.size = if quality == Quality::High { 2048 } else { 1024 };
    budget.0 = match quality {
        Quality::Low => 250,
        Quality::Medium => 550,
        _ => 900,
    };
    let mut camera = commands.entity(*camera);
    if quality == Quality::Low {
        camera.remove::<Bloom>();
    } else {
        camera.insert(Bloom::NATURAL);
    }
    let ambient_occlusion = quality == Quality::High && cfg!(not(target_arch = "wasm32"));
    #[cfg(not(target_arch = "wasm32"))]
    if ambient_occlusion {
        camera.insert(bevy::pbr::ScreenSpaceAmbientOcclusion::default());
    } else {
        camera.remove::<bevy::pbr::ScreenSpaceAmbientOcclusion>();
    }
    // L'occlusion demande de couper le MSAA (le FXAA lisse alors) ; en qualité basse, le FXAA
    // seul coûte moins cher.
    if ambient_occlusion || quality == Quality::Low {
        camera.insert((Msaa::Off, Fxaa::default()));
    } else {
        camera.insert(Msaa::Sample4);
        camera.remove::<Fxaa>();
    }
}

/// L'interface est dessinée pour 1280 × 720 : elle suit la taille de la fenêtre.
fn apply_ui_scale(
    profile: Res<Profile>,
    effective: Res<Effective>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut scale: ResMut<UiScale>,
) {
    let fit = (window.width() / 1280.0).min(window.height() / 720.0);
    // Sur un téléphone, plus gros que la simple réduction (pour le doigt et la lecture).
    let touch = if effective.touch_ui { 1.3 } else { 1.0 };
    let factor = match profile.ui_size {
        UiSize::Auto | UiSize::Normal => 1.0,
        UiSize::Small => 0.85,
        UiSize::Large => 1.2,
    };
    let wanted = (fit * touch * factor).clamp(0.5, 1.6);
    let wanted = if profile.ui_size == UiSize::Auto && !effective.touch_ui {
        wanted.min(1.0)
    } else {
        wanted
    };
    if (scale.0 - wanted).abs() > 0.01 {
        scale.0 = wanted;
    }
}

const CARD: Color = Color::srgba(0.1, 0.08, 0.16, 0.95);
const CARD_SELECTED: Color = Color::srgba(0.42, 0.2, 0.72, 0.98);

/// Une option cliquable de la page des réglages.
#[derive(Clone, Copy)]
enum Choice {
    Quality(Quality),
    Size(UiSize),
    Touch(TouchControls),
    Muted(bool),
    French(bool),
    Tutorial,
}

fn quality_name(q: Quality, french: bool) -> &'static str {
    match (q, french) {
        (Quality::Auto, _) => "Auto",
        (Quality::Low, true) => "Basse",
        (Quality::Low, false) => "Low",
        (Quality::Medium, true) => "Moyenne",
        (Quality::Medium, false) => "Medium",
        (Quality::High, true) => "Haute",
        (Quality::High, false) => "High",
    }
}

/// Une ligne de réglage : son titre et ses options (nom, choix, sélectionnée).
type Row = (String, Vec<(String, Choice, bool)>);

fn update_page(
    mut commands: Commands,
    profile: Res<Profile>,
    language: Res<Language>,
    effective: Res<Effective>,
    device: Res<Device>,
    page: Single<Entity, With<SettingsPage>>,
) {
    if !profile.is_changed() && !language.is_changed() && !effective.is_changed() {
        return;
    }
    let french = *language == Language::French;
    let t = |fr: &'static str, en: &'static str| if french { fr } else { en };
    let rows: Vec<Row> = vec![
        (
            format!(
                "{} ({} : {})",
                t("Qualité graphique", "Graphics quality"),
                t("actuelle", "current"),
                quality_name(effective.quality, french)
            ),
            [Quality::Auto, Quality::Low, Quality::Medium, Quality::High]
                .into_iter()
                .map(|q| {
                    (
                        quality_name(q, french).to_string(),
                        Choice::Quality(q),
                        profile.quality == q,
                    )
                })
                .collect(),
        ),
        (
            t("Taille de l'interface", "Interface size").to_string(),
            [
                (UiSize::Auto, "Auto", "Auto"),
                (UiSize::Small, "Petite", "Small"),
                (UiSize::Normal, "Normale", "Normal"),
                (UiSize::Large, "Grande", "Large"),
            ]
            .into_iter()
            .map(|(s, fr, en)| (t(fr, en).to_string(), Choice::Size(s), profile.ui_size == s))
            .collect(),
        ),
        (
            format!(
                "{}{}",
                t("Commandes tactiles", "Touch controls"),
                if device.touch {
                    t(" (écran tactile détecté)", " (touch screen detected)")
                } else {
                    ""
                }
            ),
            [
                (TouchControls::Auto, "Auto", "Auto"),
                (TouchControls::On, "Oui", "On"),
                (TouchControls::Off, "Non", "Off"),
            ]
            .into_iter()
            .map(|(c, fr, en)| {
                (
                    t(fr, en).to_string(),
                    Choice::Touch(c),
                    profile.touch_controls == c,
                )
            })
            .collect(),
        ),
        (
            t("Son", "Sound").to_string(),
            vec![
                (
                    t("Activé", "On").to_string(),
                    Choice::Muted(false),
                    !profile.muted,
                ),
                (
                    t("Coupé", "Off").to_string(),
                    Choice::Muted(true),
                    profile.muted,
                ),
            ],
        ),
        (
            t("Langue", "Language").to_string(),
            vec![
                ("Français".to_string(), Choice::French(true), french),
                ("English".to_string(), Choice::French(false), !french),
            ],
        ),
        (
            t("Tutoriel", "Tutorial").to_string(),
            vec![(
                t("Revoir le tutoriel", "Show the tutorial again").to_string(),
                Choice::Tutorial,
                false,
            )],
        ),
    ];
    commands.entity(*page).despawn_children();
    commands.entity(*page).with_children(|page| {
        for (label, options) in rows {
            page.spawn(Node {
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: 16.0.into(),
                        ..default()
                    },
                    Node {
                        width: Val::Px(330.0),
                        ..default()
                    },
                ));
                for (name, choice, selected) in options {
                    row.spawn((
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                            border_radius: BorderRadius::all(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(if selected { CARD_SELECTED } else { CARD }),
                    ))
                    .observe(
                        move |_: On<PointerClick>,
                              mut profile: ResMut<Profile>,
                              mut language: ResMut<Language>| match choice {
                            Choice::Quality(q) => profile.quality = q,
                            Choice::Size(s) => profile.ui_size = s,
                            Choice::Touch(c) => profile.touch_controls = c,
                            Choice::Muted(m) => profile.muted = m,
                            Choice::French(f) => {
                                *language = if f {
                                    Language::French
                                } else {
                                    Language::English
                                }
                            }
                            Choice::Tutorial => profile.tutorial_done = false,
                        },
                    )
                    .with_children(|button| {
                        button.spawn((
                            Text::new(name),
                            TextFont {
                                font_size: 15.0.into(),
                                ..default()
                            },
                        ));
                    });
                }
            });
        }
    });
}
