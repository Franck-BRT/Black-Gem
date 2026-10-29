//! Gemmes : couleurs, grades, fusion, et ce qu'elles donnent à une tour.

use crate::geom::UNITS_PER_TILE;

/// Couleur d'un cristal ou d'une composante de gemme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GemColor {
    /// Dégâts accrus.
    Red,
    /// Éclair qui rebondit sur les ennemis proches.
    Yellow,
    /// Poison : dégâts dans la durée.
    Green,
    /// Ralentissement.
    Cyan,
    /// Plusieurs cibles à la fois.
    Purple,
}

impl GemColor {
    pub const ALL: [GemColor; 5] = [
        GemColor::Red,
        GemColor::Yellow,
        GemColor::Green,
        GemColor::Cyan,
        GemColor::Purple,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Grade maximal d'une gemme.
pub const MAX_GRADE: u8 = 8;

/// Une gemme : un grade et une ou plusieurs couleurs (gemme « hybride »).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Gem {
    pub grade: u8,
    /// Ensemble des couleurs (un bit par couleur).
    colors: u8,
}

impl Gem {
    pub const fn new(color: GemColor, grade: u8) -> Self {
        Self {
            grade,
            colors: color.bit(),
        }
    }

    pub fn has(self, color: GemColor) -> bool {
        self.colors & color.bit() != 0
    }

    pub fn colors(self) -> impl Iterator<Item = GemColor> {
        GemColor::ALL.into_iter().filter(move |&c| self.has(c))
    }

    /// Couleur principale (la première), pour l'affichage.
    pub fn main_color(self) -> GemColor {
        self.colors().next().unwrap_or(GemColor::Red)
    }

    pub fn color_count(self) -> u32 {
        self.colors.count_ones()
    }

    /// Fusion de deux gemmes de même grade : grade supérieur, couleurs réunies.
    pub fn fuse(self, other: Gem) -> Option<Gem> {
        (self.grade == other.grade && self.grade < MAX_GRADE).then_some(Gem {
            grade: self.grade + 1,
            colors: self.colors | other.colors,
        })
    }

    /// Effets que la gemme donne à une tour.
    pub fn profile(self) -> TowerProfile {
        // Puissance en pour cent selon le grade : chaque fusion double le coût et multiplie
        // la puissance par ~1,75 (comme dans GemCraft, les hauts grades valent l'effort).
        const POWER: [i32; MAX_GRADE as usize] = [100, 175, 300, 520, 900, 1550, 2700, 4600];
        let power = POWER[(self.grade.clamp(1, MAX_GRADE) - 1) as usize];
        // Une gemme hybride répartit sa puissance entre ses couleurs.
        let share = 100 / self.color_count().max(1) as i32;

        let mut profile = TowerProfile {
            damage: BASE_DAMAGE * power / 100,
            range: BASE_RANGE + (self.grade as i32 - 1) * UNITS_PER_TILE * 3 / 10,
            gem: Some(self),
            ..TowerProfile::default()
        };
        if self.has(GemColor::Red) {
            profile.damage += profile.damage * 90 * share / 10_000;
        }
        if self.has(GemColor::Yellow) {
            profile.chain = ((3 * share + 50) / 100) as u32;
        }
        if self.has(GemColor::Green) {
            profile.poison_per_second = profile.damage * 50 * share / 10_000;
        }
        if self.has(GemColor::Cyan) {
            profile.slow_percent = 15 + 35 * share / 100;
        }
        if self.has(GemColor::Purple) {
            profile.extra_targets = ((2 * share + 50) / 100) as u32;
        }
        profile
    }
}

/// Dégâts d'une tour sans gemme.
pub const BASE_DAMAGE: i32 = 18;
/// Portée d'une tour sans gemme, en unités fines.
pub const BASE_RANGE: i32 = 7 * UNITS_PER_TILE;
/// Durée du poison et du ralentissement, en ticks.
pub const POISON_TICKS: u32 = 120;
pub const SLOW_TICKS: u32 = 60;
/// Portée d'un rebond d'éclair, en unités fines.
pub const CHAIN_RANGE: i32 = 5 * UNITS_PER_TILE / 2;

/// Caractéristiques d'une tour selon la gemme sertie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TowerProfile {
    pub damage: i32,
    pub range: i32,
    /// Rebonds d'éclair (jaune).
    pub chain: u32,
    /// Cibles supplémentaires (violet).
    pub extra_targets: u32,
    /// Poison infligé, en points de vie par seconde (vert).
    pub poison_per_second: i32,
    /// Ralentissement en pour cent (cyan).
    pub slow_percent: i32,
    pub gem: Option<Gem>,
}

impl Default for TowerProfile {
    /// Tour sans gemme : de simples éclats de cristal.
    fn default() -> Self {
        Self {
            damage: BASE_DAMAGE,
            range: BASE_RANGE,
            chain: 0,
            extra_targets: 0,
            poison_per_second: 0,
            slow_percent: 0,
            gem: None,
        }
    }
}

/// Ce qui circule sur les convoyeurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Crystal(GemColor),
    Gem(Gem),
    /// Éclat de cristal pris au QG par un golem : sert de munition, pas à tailler.
    Shard,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_needs_same_grade_and_mixes_colors() {
        let red = Gem::new(GemColor::Red, 1);
        let yellow = Gem::new(GemColor::Yellow, 1);
        let fused = red.fuse(yellow).unwrap();
        assert_eq!(fused.grade, 2);
        assert!(fused.has(GemColor::Red) && fused.has(GemColor::Yellow));
        assert_eq!(fused.color_count(), 2);
        assert_eq!(fused.fuse(red), None);
        assert_eq!(red.fuse(red).unwrap(), Gem::new(GemColor::Red, 2));
    }

    #[test]
    fn higher_grades_are_much_stronger() {
        let g1 = Gem::new(GemColor::Cyan, 1).profile();
        let g4 = Gem::new(GemColor::Cyan, 4).profile();
        assert!(g4.damage >= g1.damage * 5);
        assert!(g4.range > g1.range);
        assert!(g1.damage > 0 && g1.slow_percent == 50);
    }

    #[test]
    fn hybrids_dilute_their_effects() {
        let pure = Gem::new(GemColor::Yellow, 2).profile();
        let hybrid = Gem::new(GemColor::Yellow, 1)
            .fuse(Gem::new(GemColor::Purple, 1))
            .unwrap()
            .profile();
        assert_eq!(pure.chain, 3);
        assert_eq!(hybrid.chain, 2);
        assert_eq!(hybrid.extra_targets, 1);
        assert_eq!(pure.extra_targets, 0);
    }

    #[test]
    fn red_adds_damage() {
        let red = Gem::new(GemColor::Red, 1).profile();
        let green = Gem::new(GemColor::Green, 1).profile();
        assert!(red.damage > green.damage);
        assert!(green.poison_per_second > 0);
    }
}
