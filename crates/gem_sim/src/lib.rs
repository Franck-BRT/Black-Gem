//! Simulation de Black Gem, indépendante du rendu.
//!
//! Règles de ce crate :
//! - **déterministe** : même graine + mêmes ordres des joueurs = même partie, sur toutes
//!   les plateformes. Pas de flottants dans l'état de la simulation, pas d'horloge système,
//!   pas de hasard hors de [`rng`] ;
//! - **sans dépendance au moteur** : testable seul, réutilisable par un serveur ou un replay.

pub mod map;
pub mod noise;
pub mod rng;

pub use map::{Map, Terrain};

/// Fréquence de la simulation (ticks par seconde), fixe quelle que soit la vitesse d'affichage.
pub const TICKS_PER_SECOND: u32 = 30;

/// Durée d'un cycle jour/nuit, en ticks (6 minutes pour le prototype).
pub const TICKS_PER_DAY: u32 = 6 * 60 * TICKS_PER_SECOND;

/// État complet d'une expédition.
#[derive(Clone, Debug)]
pub struct Simulation {
    pub map: Map,
    pub tick: u64,
}

impl Simulation {
    pub fn new(seed: u64, map_size: u32) -> Self {
        Self {
            map: Map::generate(seed, map_size),
            // On atterrit en début de matinée.
            tick: (TICKS_PER_DAY / 4) as u64,
        }
    }

    /// Avance la simulation d'un tick.
    pub fn step(&mut self) {
        self.tick += 1;
    }

    /// Heure du jour, en millièmes de journée : 0 = minuit, 250 = lever, 500 = midi, 750 = coucher.
    pub fn time_of_day(&self) -> u32 {
        ((self.tick % TICKS_PER_DAY as u64) * 1000 / TICKS_PER_DAY as u64) as u32
    }

    /// La nuit, la Corruption attaque plus fort.
    pub fn is_night(&self) -> bool {
        !(250..750).contains(&self.time_of_day())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_night_cycle() {
        let mut sim = Simulation::new(1, 32);
        assert_eq!(sim.time_of_day(), 250);
        assert!(!sim.is_night());
        for _ in 0..TICKS_PER_DAY / 2 {
            sim.step();
        }
        assert_eq!(sim.time_of_day(), 750);
        assert!(sim.is_night());
    }
}
