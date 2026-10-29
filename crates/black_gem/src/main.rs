//! Black Gem — prototype M3 : usine, gemmes, tours, et golems programmés en Python.

mod build;
mod camera;
mod code_buffer;
mod curvature;
mod editor;
mod hud;
mod i18n;
mod palette;
mod sim;
mod sky;
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
        ))
        .run();
}
