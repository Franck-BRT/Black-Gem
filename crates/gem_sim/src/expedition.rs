//! Formats d'expédition : durée, taille de la carte, paliers de forage, difficulté.
//!
//! Le joueur choisit le format selon le temps dont il dispose ; les règles restent les mêmes.

use crate::TICKS_PER_SECOND;

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
            },
            Format::Expedition => ExpeditionConfig {
                format: self,
                map_size: 192,
                ticks_per_day: minutes(6),
                stages: 4,
                crystals_per_stage: 60,
                difficulty: 100,
            },
            Format::GreatDrill => ExpeditionConfig {
                format: self,
                map_size: 224,
                ticks_per_day: minutes(8),
                stages: 6,
                crystals_per_stage: 90,
                difficulty: 120,
            },
            Format::Colony => ExpeditionConfig {
                format: self,
                map_size: 224,
                ticks_per_day: minutes(6),
                stages: 0,
                crystals_per_stage: 0,
                difficulty: 100,
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
}

impl ExpeditionConfig {
    /// Cristaux à brûler pour extraire la Gemme Noire.
    pub fn total_crystals(&self) -> u32 {
        self.stages * self.crystals_per_stage
    }
}

/// Durée du décollage après l'extraction : il faut tenir jusque-là.
pub const EXTRACTION_TICKS: u64 = 60 * TICKS_PER_SECOND as u64;
