//! Formats d'expédition : durée, taille de la carte, paliers de forage, difficulté.
//!
//! Le joueur choisit le format selon le temps dont il dispose ; les règles restent les mêmes.

use crate::{
    STARTING_CRYSTALS, TICKS_PER_SECOND,
    building::{CONVEYOR_TICKS, CORE_DRILL_PERIOD, CRYSTALS_PER_GEM, DRILL_PERIOD},
    golem::{GOLEM_COST, GOLEM_SPEED},
};

/// Règles ajustables par les améliorations permanentes (recherche de l'Arche).
/// Elles font partie de la configuration de l'expédition : en multijoueur, tous les joueurs
/// les partagent, et la partie reste déterministe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bonuses {
    pub starting_crystals: u32,
    /// Points de vie supplémentaires du QG.
    pub hq_hp: i32,
    /// Ticks entre deux cristaux d'une foreuse.
    pub drill_period: u32,
    /// Ticks pour traverser une case de convoyeur.
    pub conveyor_ticks: u32,
    /// Cristaux d'une couleur pour tailler une gemme.
    pub crystals_per_gem: u8,
    /// Ticks pour brûler un cristal dans le Grand foreur.
    pub core_drill_period: u32,
    /// Dégâts des tours, en pour cent.
    pub tower_damage: i32,
    /// Portée supplémentaire des tours, en unités fines.
    pub tower_range: i32,
    /// Vitesse des golems, en unités fines par tick.
    pub golem_speed: i32,
    pub golem_cost: u32,
    /// Une gemme de grade 2 offerte à l'atterrissage.
    pub starting_gem: bool,
}

impl Default for Bonuses {
    fn default() -> Self {
        Self {
            starting_crystals: STARTING_CRYSTALS,
            hq_hp: 0,
            drill_period: DRILL_PERIOD,
            conveyor_ticks: CONVEYOR_TICKS,
            crystals_per_gem: CRYSTALS_PER_GEM,
            core_drill_period: CORE_DRILL_PERIOD,
            tower_damage: 100,
            tower_range: 0,
            golem_speed: GOLEM_SPEED,
            golem_cost: GOLEM_COST,
            starting_gem: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// ~15 min : petite carte, journées courtes, deux paliers.
    Scout,
    /// 30–45 min : le format standard.
    Expedition,
    /// 60–90 min : grande carte, six paliers.
    GreatDrill,
    /// Sans fin : survivre et construire, pas de Cœur à forer.
    Colony,
}

impl Format {
    pub const ALL: [Format; 4] = [
        Format::Scout,
        Format::Expedition,
        Format::GreatDrill,
        Format::Colony,
    ];

    pub fn config(self) -> ExpeditionConfig {
        let minutes = |m: u32| m * 60 * TICKS_PER_SECOND;
        match self {
            Format::Scout => ExpeditionConfig {
                format: self,
                map_size: 128,
                ticks_per_day: minutes(3),
                stages: 2,
                crystals_per_stage: 30,
                difficulty: 70,
                bonuses: Bonuses::default(),
            },
            Format::Expedition => ExpeditionConfig {
                format: self,
                map_size: 192,
                ticks_per_day: minutes(6),
                stages: 4,
                crystals_per_stage: 60,
                difficulty: 100,
                bonuses: Bonuses::default(),
            },
            Format::GreatDrill => ExpeditionConfig {
                format: self,
                map_size: 224,
                ticks_per_day: minutes(8),
                stages: 6,
                crystals_per_stage: 90,
                difficulty: 120,
                bonuses: Bonuses::default(),
            },
            Format::Colony => ExpeditionConfig {
                format: self,
                map_size: 224,
                ticks_per_day: minutes(6),
                stages: 0,
                crystals_per_stage: 0,
                difficulty: 100,
                bonuses: Bonuses::default(),
            },
        }
    }
}

/// Paramètres d'une expédition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpeditionConfig {
    pub format: Format,
    /// Nombre de cases par côté.
    pub map_size: u32,
    pub ticks_per_day: u32,
    /// Paliers de forage (chacun déclenche une vague majeure) ; 0 = pas de Cœur.
    pub stages: u32,
    /// Cristaux à brûler dans le Grand foreur pour franchir un palier.
    pub crystals_per_stage: u32,
    /// Force des vagues, en pour cent.
    pub difficulty: u32,
    pub bonuses: Bonuses,
}

impl ExpeditionConfig {
    /// Cristaux à brûler pour extraire la Gemme Noire.
    pub fn total_crystals(&self) -> u32 {
        self.stages * self.crystals_per_stage
    }
}

/// Durée du décollage après l'extraction : il faut tenir jusque-là.
pub const EXTRACTION_TICKS: u64 = 60 * TICKS_PER_SECOND as u64;
