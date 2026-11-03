//! Black Gem — prototype M7 : les golems se programment en Python ou en Rust (vérifié comme
//! par rustc : types, ownership, emprunts), passe graphique, l'Arche, expéditions de 15 min
//! à plus d'une heure, forage du Cœur, usine, gemmes, tours.

mod audio;
mod build;
mod camera;
mod code_buffer;
mod curvature;
mod editor;
mod hud;
mod i18n;
mod mission;
mod overlay;
mod palette;
mod particles;
mod profile;
mod sim;
mod sky;
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
        ))
        .run();
}
