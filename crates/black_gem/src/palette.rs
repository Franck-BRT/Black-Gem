//! Couleurs des gemmes, partagées par le décor, les bâtiments, les tirs et l'interface.

use bevy::prelude::*;
use gem_sim::GemColor;

/// Teinte pure d'une couleur de gemme (à multiplier pour obtenir une lueur HDR).
pub fn gem_rgb(color: GemColor) -> Vec3 {
    match color {
        GemColor::Red => Vec3::new(1.0, 0.12, 0.08),
        GemColor::Yellow => Vec3::new(1.0, 0.78, 0.08),
        GemColor::Green => Vec3::new(0.15, 1.0, 0.25),
        GemColor::Cyan => Vec3::new(0.12, 0.85, 1.0),
        GemColor::Purple => Vec3::new(0.62, 0.18, 1.0),
    }
}

pub fn gem_glow(color: GemColor, strength: f32) -> LinearRgba {
    let c = gem_rgb(color) * strength;
    LinearRgba::rgb(c.x, c.y, c.z)
}

pub fn gem_ui(color: GemColor) -> Color {
    let c = gem_rgb(color);
    Color::srgb(c.x, c.y, c.z)
}
