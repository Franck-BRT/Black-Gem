//! Black Gem — prototype M10 (systèmes RTS : énergie, armée, héros, brouillard de guerre, factions ; jouable au doigt) : les golems se programment en Python ou en Rust (vérifié comme
//! par rustc : types, ownership, emprunts, traits, itérateurs) et se coordonnent, passe graphique, l'Arche, expéditions de 15 min
//! à plus d'une heure, forage du Cœur, usine, gemmes, tours.

mod army;
mod audio;
mod blocks;
mod build;
mod camera;
mod code_buffer;
mod curvature;
mod editor;
mod fog;
mod hud;
mod i18n;
mod mission;
mod overlay;
mod palette;
mod particles;
mod profile;
mod settings;
mod sim;
mod sky;
mod touch;
mod tutorial;
mod view;
mod world;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Black Gem".into(),
                // Version web : le jeu remplit le <canvas id="bevy"> de la page.
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            i18n::I18nPlugin,
            sim::SimPlugin,
            curvature::CurvaturePlugin,
            world::WorldPlugin,
            camera::RtsCameraPlugin,
            sky::SkyPlugin,
            build::BuildPlugin,
            view::ViewPlugin,
            hud::HudPlugin,
            editor::EditorPlugin,
            mission::MissionPlugin,
            profile::ProfilePlugin,
            particles::ParticlesPlugin,
        ))
        .add_plugins((
            audio::SoundPlugin,
            overlay::OverlayPlugin,
            tutorial::TutorialPlugin,
            settings::SettingsPlugin,
            touch::TouchPlugin,
            blocks::BlocksPlugin,
            army::ArmyPlugin,
            fog::FogPlugin,
        ))
        .run();
}
