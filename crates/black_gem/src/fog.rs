//! Brouillard de guerre : le monde inconnu reste noir, le terrain déjà vu est assombri et
//! seuls les abords des bâtiments, des unités et des golems sont éclairés (ennemis visibles).
//!
//! Une petite texture (une valeur par case, filtrée linéairement pour des bords doux) est
//! lue par le fragment shader de tous les matériaux du monde (`fogged.wesl`, `water.wesl`).

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use gem_sim::{Pos, Tile};

use crate::{
    curvature::{CurvedMaterial, WaterMaterial},
    sim::Sim,
    world,
};

/// Luminosité d'une case : vue maintenant, déjà vue, jamais vue.
const VISIBLE: u8 = 255;
const EXPLORED: u8 = 105;
const UNKNOWN: u8 = 2;
/// Mise à jour au plus tous ces intervalles (secondes réelles).
const REFRESH_SECONDS: f32 = 0.2;

#[derive(Resource, Default)]
pub struct FogOfWar {
    image: Option<Handle<Image>>,
    size: i32,
    visible: Vec<bool>,
    /// Planète (graine, taille) de la texture actuelle.
    planet: Option<(u64, u32)>,
    last_tick: Option<u64>,
    last_refresh: f32,
}

impl FogOfWar {
    /// La case est-elle vue en ce moment ?
    pub fn visible(&self, tile: Tile) -> bool {
        (0..self.size).contains(&tile.x)
            && (0..self.size).contains(&tile.y)
            && self
                .visible
                .get((tile.y * self.size + tile.x) as usize)
                .copied()
                .unwrap_or(true)
    }

    pub fn visible_pos(&self, pos: Pos) -> bool {
        self.visible(pos.tile())
    }
}

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWar>().add_systems(
            PostUpdate,
            update_fog.before(crate::curvature::update_curvature),
        );
    }
}

fn update_fog(
    sim: Res<Sim>,
    time: Res<Time<Real>>,
    mut fog: ResMut<FogOfWar>,
    mut images: ResMut<Assets<Image>>,
    mut curved: ResMut<Assets<CurvedMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
) {
    let now = time.elapsed_secs();
    let planet = (sim.map.seed, sim.map.size);
    let new_planet = fog.planet != Some(planet);
    let new_tick = fog.last_tick != Some(sim.tick);
    if !new_planet && (!new_tick || now - fog.last_refresh < REFRESH_SECONDS) {
        return;
    }
    fog.last_refresh = now;
    fog.last_tick = Some(sim.tick);
    let size = sim.map.size as i32;
    if new_planet {
        fog.planet = Some(planet);
        fog.size = size;
        let mut image = Image::new_fill(
            Extent3d {
                width: size as u32,
                height: size as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[UNKNOWN],
            TextureFormat::R8Unorm,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        image.sampler = ImageSampler::linear();
        fog.image = Some(images.add(image));
    }

    // Ce qui est vu maintenant.
    let mut visible = vec![false; (size * size) as usize];
    for (pos, radius) in sim.vision_sources() {
        let center = pos.tile();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let (x, y) = (center.x + dx, center.y + dy);
                if dx * dx + dy * dy <= radius * radius
                    && (0..size).contains(&x)
                    && (0..size).contains(&y)
                {
                    visible[(y * size + x) as usize] = true;
                }
            }
        }
    }
    let handle = fog.image.clone().unwrap();
    if let Some(mut image) = images.get_mut(&handle) {
        let data = image.data.get_or_insert_with(Vec::new);
        data.resize((size * size) as usize, UNKNOWN);
        for y in 0..size {
            for x in 0..size {
                let i = (y * size + x) as usize;
                data[i] = if visible[i] {
                    VISIBLE
                } else if sim.is_explored(Tile::new(x, y)) {
                    EXPLORED
                } else {
                    UNKNOWN
                };
            }
        }
    }
    fog.visible = visible;

    // Les matériaux relisent la texture (et ceux créés depuis la reçoivent).
    let params = Vec4::new(
        world::half_extent(&sim.map),
        1.0 / (size as f32 * world::TILE_SIZE),
        1.0,
        0.0,
    );
    for (_, material) in curved.iter_mut() {
        material.extension.fog = Some(handle.clone());
        material.extension.fog_params = params;
    }
    for (_, material) in waters.iter_mut() {
        material.extension.fog = Some(handle.clone());
        material.extension.fog_params = params;
    }
}
