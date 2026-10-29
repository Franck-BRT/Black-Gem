//! Génération procédurale d'une zone d'expédition.

use crate::{
    gem::GemColor,
    noise::{NOISE_ONE, fbm},
    rng::hash2,
};

/// Part des sommets sous le niveau de l'eau (en %).
const WATER_PERCENTILE: usize = 12;
/// Part des sommets sous la limite de la roche (en %).
const ROCK_PERCENTILE: usize = 86;
/// Le plateau d'atterrissage domine toujours l'eau d'au moins cette hauteur (cm).
const LANDING_ABOVE_WATER_CM: i32 = 150;
/// Rayon du plateau d'atterrissage, en cases (zone parfaitement plate).
pub const LANDING_RADIUS: i32 = 12;
/// Rayon de raccord entre le plateau et le terrain naturel, en cases.
pub const LANDING_BLEND_RADIUS: i32 = 26;

const MAX_HEIGHT_CM: i64 = 1500;

/// Nature d'une case de terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Terrain {
    Plain,
    Rock,
    Water,
    /// Filon de cristal brut : la matière première des gemmes.
    CrystalVein,
    /// Gisement de minerai métallique : la matière première des machines.
    Ore,
}

impl Terrain {
    pub const ALL: [Terrain; 5] = [
        Terrain::Plain,
        Terrain::Rock,
        Terrain::Water,
        Terrain::CrystalVein,
        Terrain::Ore,
    ];

    /// Peut-on construire dessus ?
    pub fn is_buildable(self) -> bool {
        !matches!(self, Terrain::Water)
    }

    /// La Corruption peut-elle y passer ?
    pub fn is_walkable(self) -> bool {
        !matches!(self, Terrain::Water)
    }
}

/// Une zone d'expédition : grille carrée de cases, avec une hauteur par sommet.
#[derive(Clone, Debug)]
pub struct Map {
    pub seed: u64,
    /// Nombre de cases par côté.
    pub size: u32,
    /// Niveau de l'eau, en centimètres.
    pub water_level_cm: i32,
    /// Hauteur du plateau d'atterrissage (centre de la carte), en centimètres.
    pub landing_height_cm: i32,
    /// Hauteurs des sommets, en centimètres : `(size + 1)²` valeurs, ligne par ligne.
    heights: Vec<i32>,
    /// Terrain de chaque case : `size²` valeurs, ligne par ligne.
    tiles: Vec<Terrain>,
}

/// Niveaux de référence d'une carte, déduits de la répartition de ses hauteurs
/// (chaque planète a ainsi à peu près autant d'eau et de roche, quel que soit son relief).
struct Levels {
    water: i32,
    rock: i32,
    landing: i32,
}

impl Map {
    pub fn generate(seed: u64, size: u32) -> Self {
        let s = size as i32;
        let center = s / 2;
        let verts = size as usize + 1;

        let mut heights = Vec::with_capacity(verts * verts);
        for y in 0..=s {
            for x in 0..=s {
                heights.push(natural_height(seed, x, y));
            }
        }

        let mut sorted = heights.clone();
        sorted.sort_unstable();
        let percentile = |p: usize| sorted[(sorted.len() - 1) * p / 100];
        let water = percentile(WATER_PERCENTILE);
        let levels = Levels {
            water,
            rock: percentile(ROCK_PERCENTILE),
            landing: percentile(50).max(water + LANDING_ABOVE_WATER_CM),
        };

        for y in 0..=s {
            for x in 0..=s {
                let h = &mut heights[y as usize * verts + x as usize];
                *h = flatten_landing_zone(*h, x, y, center, levels.landing);
            }
        }

        let mut tiles = Vec::with_capacity((size * size) as usize);
        for y in 0..s {
            for x in 0..s {
                let i = y as usize * verts + x as usize;
                let corners = [
                    heights[i],
                    heights[i + 1],
                    heights[i + verts],
                    heights[i + verts + 1],
                ];
                let lowest = *corners.iter().min().unwrap();
                let highest = *corners.iter().max().unwrap();
                tiles.push(classify(seed, x, y, center, lowest, highest, &levels));
            }
        }

        Self {
            seed,
            size,
            water_level_cm: levels.water,
            landing_height_cm: levels.landing,
            heights,
            tiles,
        }
    }

    /// Hauteur d'un sommet de la grille (`0..=size` sur chaque axe), en centimètres.
    pub fn vertex_height(&self, x: u32, y: u32) -> i32 {
        self.heights[(y * (self.size + 1) + x) as usize]
    }

    pub fn tile(&self, x: u32, y: u32) -> Terrain {
        self.tiles[(y * self.size + x) as usize]
    }

    /// Couleur du cristal d'un filon. Les couleurs forment de grandes régions aux bords
    /// irréguliers : chaque couleur se mérite, il faut aller la chercher.
    pub fn vein_color(&self, x: u32, y: u32) -> GemColor {
        const REGION: i32 = 36;
        let (x, y) = (x as i32, y as i32);
        let warp = |salt: u64| {
            ((fbm(self.seed ^ salt, x, y, &[24]) - NOISE_ONE / 2) * 2 * REGION as i64
                / 3
                / NOISE_ONE) as i32
        };
        let cx = (x + warp(0xA11CE)).div_euclid(REGION);
        let cy = (y + warp(0xB0B)).div_euclid(REGION);
        GemColor::ALL[(hash2(self.seed ^ 0xC0_1025, cx, cy) % 5) as usize]
    }

    /// Hauteur moyenne d'une case, en centimètres.
    pub fn tile_height(&self, x: u32, y: u32) -> i32 {
        (self.vertex_height(x, y)
            + self.vertex_height(x + 1, y)
            + self.vertex_height(x, y + 1)
            + self.vertex_height(x + 1, y + 1))
            / 4
    }

    pub fn tiles(&self) -> impl Iterator<Item = (u32, u32, Terrain)> + '_ {
        let size = self.size;
        self.tiles
            .iter()
            .enumerate()
            .map(move |(i, &t)| (i as u32 % size, i as u32 / size, t))
    }

    /// Empreinte de la carte : sert à vérifier que deux machines ont généré la même
    /// planète (tests multi-plateformes, et plus tard détection de désynchronisation en multijoueur).
    pub fn checksum(&self) -> u64 {
        // FNV-1a
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        let mut feed = |v: u64| {
            for byte in v.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        feed(self.water_level_cm as u64);
        feed(self.landing_height_cm as u64);
        for &h in &self.heights {
            feed(h as u64);
        }
        for &t in &self.tiles {
            feed(t as u64);
        }
        hash
    }
}

fn distance_sq(x: i32, y: i32, center: i32) -> i32 {
    (x - center).pow(2) + (y - center).pow(2)
}

fn natural_height(seed: u64, x: i32, y: i32) -> i32 {
    let base = fbm(seed, x, y, &[96, 48, 24, 12, 6]);
    (base * MAX_HEIGHT_CM / NOISE_ONE) as i32
}

/// Le plateau d'atterrissage est plat et se raccorde en douceur au terrain naturel.
fn flatten_landing_zone(natural: i32, x: i32, y: i32, center: i32, landing: i32) -> i32 {
    let d2 = distance_sq(x, y, center);
    let inner = LANDING_RADIUS * LANDING_RADIUS;
    let outer = LANDING_BLEND_RADIUS * LANDING_BLEND_RADIUS;
    if d2 <= inner {
        landing
    } else if d2 < outer {
        let t = (d2 - inner) as i64 * 1024 / (outer - inner) as i64;
        let t = t * t * (3 * 1024 - 2 * t) / (1024 * 1024); // atténuation douce
        landing + ((natural - landing) as i64 * t / 1024) as i32
    } else {
        natural
    }
}

fn classify(
    seed: u64,
    x: i32,
    y: i32,
    center: i32,
    lowest: i32,
    highest: i32,
    levels: &Levels,
) -> Terrain {
    let mean = (lowest + highest) / 2;
    if mean < levels.water {
        return Terrain::Water;
    }
    if distance_sq(x, y, center) <= LANDING_RADIUS * LANDING_RADIUS {
        return Terrain::Plain;
    }

    // Les filons de cristal suivent les « crêtes » d'un second bruit : de longues veines sinueuses.
    let vein = fbm(seed ^ 0xC0FF_EE00_0000_0001, x, y, &[56, 28]);
    let near_ridge = (vein - NOISE_ONE / 2).abs() < NOISE_ONE / 70;
    if near_ridge && mean > levels.water + 40 {
        return Terrain::CrystalVein;
    }

    if highest - lowest > 45 || mean > levels.rock {
        return Terrain::Rock;
    }

    let ore = fbm(seed ^ 0x0BAD_5EED_0000_0002, x, y, &[28, 14, 7]);
    if ore > NOISE_ONE * 71 / 100 {
        return Terrain::Ore;
    }

    Terrain::Plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = Map::generate(1234, 96);
        let b = Map::generate(1234, 96);
        assert_eq!(a.checksum(), b.checksum());
    }

    #[test]
    fn different_seeds_give_different_maps() {
        assert_ne!(
            Map::generate(1, 96).checksum(),
            Map::generate(2, 96).checksum()
        );
    }

    /// Empreinte de référence : si ce test échoue sur une seule plateforme, la génération
    /// n'est plus déterministe et le multijoueur se désynchroniserait. Si la génération
    /// change volontairement, mettre à jour la valeur.
    #[test]
    fn checksum_is_stable_across_platforms() {
        assert_eq!(
            Map::generate(2026, 128).checksum(),
            12_485_880_301_749_087_181
        );
    }

    #[test]
    fn landing_zone_is_flat_and_buildable() {
        let map = Map::generate(99, 128);
        let c = map.size / 2;
        let r = LANDING_RADIUS as u32 / 2;
        for y in c - r..c + r {
            for x in c - r..c + r {
                assert_eq!(map.tile(x, y), Terrain::Plain);
                assert_eq!(map.vertex_height(x, y), map.landing_height_cm);
            }
        }
    }

    #[test]
    fn every_vein_color_appears() {
        let map = Map::generate(2026, 192);
        for color in GemColor::ALL {
            let count = map
                .tiles()
                .filter(|&(x, y, t)| t == Terrain::CrystalVein && map.vein_color(x, y) == color)
                .count();
            assert!(count > 20, "{color:?} : seulement {count} cases");
        }
    }

    #[test]
    fn every_terrain_kind_appears() {
        let map = Map::generate(7, 256);
        for kind in Terrain::ALL {
            let count = map.tiles().filter(|&(_, _, t)| t == kind).count();
            assert!(count > 50, "{kind:?} : seulement {count} cases");
        }
    }
}
