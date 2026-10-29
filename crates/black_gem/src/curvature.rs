//! Courbure de l'horizon façon « petite planète ».
//!
//! Le monde reste une grille plate (simple pour construire et pour la simulation) ;
//! c'est le vertex shader qui fait « tomber » le sol avec la distance au point visé.
//! La courbure dépend de l'angle de la caméra : invisible vue de dessus (pour
//! construire), elle apparaît quand on baisse la caméra vers l'horizon ou qu'on dézoome.

use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

use crate::camera::RtsCamera;

pub type CurvedMaterial = ExtendedMaterial<StandardMaterial, Curvature>;

/// Force maximale de la courbure : le sol descend de `k × distance²`.
const MAX_CURVATURE: f32 = 0.0016;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Curvature {
    /// xy : point visé (x, z du monde) ; z : force ; w : inutilisé (alignement 16 octets pour WebGL2).
    #[uniform(100)]
    pub params: Vec4,
}

impl MaterialExtension for Curvature {
    fn vertex_shader() -> ShaderRef {
        "embedded://black_gem/shaders/curved_mesh.wesl".into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://black_gem/shaders/curved_prepass.wesl".into()
    }
}

/// Valeur courante de la courbure, partagée par tous les matériaux du monde.
#[derive(Resource, Default)]
pub struct CurrentCurvature {
    pub params: Vec4,
}

pub struct CurvaturePlugin;

impl Plugin for CurvaturePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/curved_mesh.wesl");
        bevy::asset::embedded_asset!(app, "shaders/curved_prepass.wesl");
        app.add_plugins(MaterialPlugin::<CurvedMaterial>::default())
            .init_resource::<CurrentCurvature>()
            .add_systems(PostUpdate, update_curvature);
    }
}

/// Crée un matériau courbé à partir d'un `StandardMaterial` classique.
pub fn curved(base: StandardMaterial) -> CurvedMaterial {
    CurvedMaterial {
        base,
        extension: Curvature::default(),
    }
}

/// Force de la courbure pour un angle (radians au-dessus de l'horizon) et une distance de caméra.
pub fn curvature_strength(pitch: f32, distance: f32) -> f32 {
    let by_angle = 1.0 - smoothstep(30f32.to_radians(), 75f32.to_radians(), pitch);
    let by_zoom = 0.35 + 0.65 * smoothstep(20.0, 180.0, distance);
    MAX_CURVATURE * by_angle * by_zoom
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn update_curvature(
    camera: Single<&RtsCamera>,
    mut current: ResMut<CurrentCurvature>,
    mut materials: ResMut<Assets<CurvedMaterial>>,
) {
    let params = Vec4::new(
        camera.focus.x,
        camera.focus.z,
        curvature_strength(camera.pitch, camera.distance),
        0.0,
    );
    if params == current.params {
        return;
    }
    current.params = params;
    // Peu de matériaux partagés (instanciation automatique) : les mettre tous à jour est bon marché.
    for (_, material) in materials.iter_mut() {
        material.extension.params = params;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_when_seen_from_above() {
        assert_eq!(curvature_strength(80f32.to_radians(), 100.0), 0.0);
    }

    #[test]
    fn stronger_near_the_horizon_and_when_zoomed_out() {
        let low_close = curvature_strength(20f32.to_radians(), 20.0);
        let low_far = curvature_strength(20f32.to_radians(), 200.0);
        let high_far = curvature_strength(60f32.to_radians(), 200.0);
        assert!(low_far > low_close);
        assert!(low_far > high_far);
        assert!(high_far > 0.0);
    }
}
