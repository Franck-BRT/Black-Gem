//! Simulation de Black Gem, indépendante du rendu.
//!
//! Règles de ce crate :
//! - **déterministe** : même graine + mêmes ordres des joueurs = même partie, sur toutes
//!   les plateformes. Pas de flottants dans l'état de la simulation, pas d'horloge système,
//!   pas de hasard hors de [`rng`] ;
//! - **sans dépendance au moteur** : testable seul, réutilisable par un serveur ou un replay ;
//! - **piloté par des ordres** ([`Command`]) : le joueur ne modifie jamais l'état directement.
//!   En multijoueur (lockstep), ce sont ces ordres qui circuleront sur le réseau.

pub mod building;
pub mod enemy;
pub mod geom;
pub mod map;
pub mod noise;
pub mod rng;

use std::collections::VecDeque;

pub use building::{BuildError, Building, BuildingKind, BuildingState};
pub use enemy::Enemy;
pub use geom::{Dir, Pos, Tile, UNITS_PER_TILE};
pub use map::{Map, Terrain};

use building::{BuildingState as State, Buildings, HQ_SIZE};
use enemy::{FlowField, SPAWN_INTERVAL, choose_nests, wave_spec};

/// Fréquence de la simulation (ticks par seconde), fixe quelle que soit la vitesse d'affichage.
pub const TICKS_PER_SECOND: u32 = 30;

/// Durée d'un cycle jour/nuit, en ticks (6 minutes pour le prototype).
pub const TICKS_PER_DAY: u32 = 6 * 60 * TICKS_PER_SECOND;

/// Cristaux disponibles à l'atterrissage.
pub const STARTING_CRYSTALS: u32 = 50;

/// Nombre de nuits à tenir pour gagner l'expédition (prototype).
pub const WAVES_TO_WIN: u32 = 3;

/// Durée de conservation d'un tir, en ticks (le rendu peut sauter des ticks en vitesse rapide).
const SHOT_LIFETIME: u64 = 30;

/// Ordre donné par un joueur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Build {
        kind: BuildingKind,
        tile: Tile,
        dir: Dir,
    },
    Demolish {
        tile: Tile,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Defeat,
}

/// Un tir (pour l'affichage des rayons).
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub tick: u64,
    pub from: Pos,
    pub to: Pos,
    pub from_hq: bool,
}

/// État complet d'une expédition.
#[derive(Clone, Debug)]
pub struct Simulation {
    pub map: Map,
    pub tick: u64,
    pub buildings: Buildings,
    pub enemies: Vec<Enemy>,
    /// Cristaux stockés au QG (la monnaie de construction).
    pub stock: u32,
    pub nests: Vec<Tile>,
    /// Nombre de vagues déjà lancées.
    pub wave: u32,
    pub kills: u32,
    pub shots: Vec<Shot>,
    pub outcome: Option<Outcome>,
    field: FlowField,
    /// Apparitions prévues : (tick, nid), dans l'ordre chronologique.
    pending_spawns: VecDeque<(u64, usize)>,
    commands: Vec<Command>,
    next_enemy_id: u32,
}

impl Simulation {
    pub fn new(seed: u64, map_size: u32) -> Self {
        let map = Map::generate(seed, map_size);
        let mut buildings = Buildings::new(map_size);
        let corner = (map_size as i32 - HQ_SIZE) / 2;
        let hq_tile = Tile::new(corner, corner);
        buildings.place(BuildingKind::Hq, hq_tile, Dir::South);

        let hq_tiles: Vec<Tile> = (0..HQ_SIZE)
            .flat_map(|dy| (0..HQ_SIZE).map(move |dx| Tile::new(corner + dx, corner + dy)))
            .collect();
        let field = FlowField::toward(&map, &hq_tiles);
        let nests = choose_nests(&map, &field);

        Self {
            map,
            // On atterrit en début de matinée.
            tick: (TICKS_PER_DAY / 4) as u64,
            buildings,
            enemies: Vec::new(),
            stock: STARTING_CRYSTALS,
            nests,
            wave: 0,
            kills: 0,
            shots: Vec::new(),
            outcome: None,
            field,
            pending_spawns: VecDeque::new(),
            commands: Vec::new(),
            next_enemy_id: 1,
        }
    }

    /// Heure du jour, en millièmes de journée : 0 = minuit, 250 = lever, 500 = midi, 750 = coucher.
    pub fn time_of_day(&self) -> u32 {
        ((self.tick % TICKS_PER_DAY as u64) * 1000 / TICKS_PER_DAY as u64) as u32
    }

    /// La nuit, la Corruption attaque.
    pub fn is_night(&self) -> bool {
        !(250..750).contains(&self.time_of_day())
    }

    /// Ticks avant la prochaine tombée de la nuit.
    pub fn ticks_until_nightfall(&self) -> u64 {
        let day = TICKS_PER_DAY as u64;
        let dusk = day * 3 / 4;
        (dusk + day - self.tick % day) % day
    }

    pub fn hq(&self) -> &Building {
        self.buildings.hq().expect("le QG existe toujours")
    }

    /// Une construction serait-elle acceptée ?
    pub fn check_build(&self, kind: BuildingKind, tile: Tile) -> Result<(), BuildError> {
        self.buildings.check_site(&self.map, kind, tile)?;
        if self.stock < kind.cost() {
            return Err(BuildError::NotEnoughCrystals);
        }
        Ok(())
    }

    /// Enregistre un ordre ; il sera exécuté au début du prochain tick.
    pub fn queue(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Avance la simulation d'un tick.
    pub fn step(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        for command in std::mem::take(&mut self.commands) {
            self.apply(command);
        }
        self.update_waves();
        self.buildings.update_logistics(&mut self.stock);
        self.fire_weapons();
        self.update_enemies();
        self.shots.retain(|s| s.tick + SHOT_LIFETIME > self.tick);
        self.tick += 1;
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Build { kind, tile, dir } => {
                if kind != BuildingKind::Hq && self.check_build(kind, tile).is_ok() {
                    self.stock -= kind.cost();
                    self.buildings.place(kind, tile, dir);
                }
            }
            Command::Demolish { tile } => {
                if let Some(b) = self.buildings.at(tile)
                    && b.kind != BuildingKind::Hq
                {
                    let id = b.id;
                    let removed = self.buildings.remove(id).unwrap();
                    self.stock += removed.kind.cost() / 2;
                }
            }
        }
    }

    fn update_waves(&mut self) {
        let day = TICKS_PER_DAY as u64;
        if self.tick % day == day * 3 / 4 && self.wave < WAVES_TO_WIN {
            self.wave += 1;
            let spec = wave_spec(self.wave);
            for i in 0..spec.per_nest as u64 {
                for nest in 0..self.nests.len() {
                    self.pending_spawns
                        .push_back((self.tick + i * SPAWN_INTERVAL + nest as u64 * 7, nest));
                }
            }
        }
        while let Some(&(at, nest)) = self.pending_spawns.front() {
            if at > self.tick {
                break;
            }
            self.pending_spawns.pop_front();
            let spec = wave_spec(self.wave);
            let pos = self.nests[nest].center();
            self.enemies.push(Enemy {
                id: self.next_enemy_id,
                pos,
                prev_pos: pos,
                hp: spec.hp,
                max_hp: spec.hp,
                speed: spec.speed,
                attack_cooldown: 0,
            });
            self.next_enemy_id += 1;
        }
    }

    fn fire_weapons(&mut self) {
        for id in self.buildings.ids() {
            let building = self.buildings.get_mut(id).unwrap();
            let Some(weapon) = building.weapon() else {
                continue;
            };
            let center = building.center();
            let from_hq = building.kind == BuildingKind::Hq;
            let (cooldown, charge) = match &mut building.state {
                State::Hq { cooldown } => (cooldown, None),
                State::Tower {
                    cooldown, charge, ..
                } => (cooldown, Some(charge)),
                _ => continue,
            };
            if *cooldown > 0 {
                *cooldown -= 1;
                continue;
            }
            if charge.as_ref().is_some_and(|c| **c == 0) {
                continue;
            }
            let range_sq = (weapon.range as i64).pow(2);
            let target = self
                .enemies
                .iter_mut()
                .filter(|e| e.hp > 0 && e.pos.distance_sq(center) <= range_sq)
                .min_by_key(|e| (e.pos.distance_sq(center), e.id));
            if let Some(enemy) = target {
                enemy.hp -= weapon.damage;
                *cooldown = weapon.cooldown;
                if let Some(charge) = charge {
                    *charge -= 1;
                }
                self.shots.push(Shot {
                    tick: self.tick,
                    from: center,
                    to: enemy.pos,
                    from_hq,
                });
            }
        }
    }

    fn update_enemies(&mut self) {
        let before = self.enemies.len();
        self.enemies.retain(|e| e.hp > 0);
        self.kills += (before - self.enemies.len()) as u32;

        let hq_center = self.hq().center();
        let damage: i32 = self
            .enemies
            .iter_mut()
            .map(|e| e.update(&self.field, hq_center))
            .sum();
        if damage > 0 {
            let hq_id = self.hq().id;
            let hq = self.buildings.get_mut(hq_id).unwrap();
            hq.hp -= damage;
            if hq.hp <= 0 {
                self.outcome = Some(Outcome::Defeat);
            }
        }
        if self.wave == WAVES_TO_WIN && self.pending_spawns.is_empty() && self.enemies.is_empty() {
            self.outcome = Some(Outcome::Victory);
        }
    }

    /// Empreinte de l'état complet : deux machines qui jouent la même partie doivent
    /// obtenir la même valeur à chaque tick (détection de désynchronisation en multijoueur).
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = self.map.checksum();
        let mut feed = |v: i64| {
            hash = rng::mix64(hash ^ v as u64);
        };
        feed(self.tick as i64);
        feed(self.stock as i64);
        feed(self.wave as i64);
        feed(self.kills as i64);
        for b in self.buildings.iter() {
            feed(b.id as i64);
            feed(b.hp as i64);
            feed(b.tile.x as i64);
            feed(b.tile.y as i64);
            let state = match b.state {
                State::Hq { cooldown } => [cooldown, 0, 0],
                State::Drill { timer, buffer } => [timer, buffer, 0],
                State::Conveyor { item } => [item.unwrap_or(u32::MAX), 0, 0],
                State::Tower {
                    crystals,
                    charge,
                    cooldown,
                } => [crystals, charge, cooldown],
            };
            for v in state {
                feed(v as i64);
            }
        }
        for e in &self.enemies {
            feed(e.id as i64);
            feed(e.pos.x as i64);
            feed(e.pos.y as i64);
            feed(e.hp as i64);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sim: &mut Simulation, ticks: u64) {
        for _ in 0..ticks {
            sim.step();
        }
    }

    #[test]
    fn day_night_cycle() {
        let mut sim = Simulation::new(1, 64);
        assert_eq!(sim.time_of_day(), 250);
        assert!(!sim.is_night());
        assert_eq!(sim.ticks_until_nightfall(), (TICKS_PER_DAY / 2) as u64);
        run(&mut sim, (TICKS_PER_DAY / 2) as u64);
        assert_eq!(sim.time_of_day(), 750);
        assert!(sim.is_night());
    }

    #[test]
    fn building_costs_crystals_and_demolishing_refunds_half() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let tile = Tile::new(hq.x - 3, hq.y);
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        sim.step();
        assert_eq!(sim.stock, STARTING_CRYSTALS - BuildingKind::Tower.cost());
        sim.queue(Command::Demolish { tile });
        sim.step();
        assert_eq!(
            sim.stock,
            STARTING_CRYSTALS - BuildingKind::Tower.cost() / 2
        );
        assert!(sim.buildings.at(tile).is_none());
    }

    #[test]
    fn hq_cannot_be_demolished_and_stock_cannot_go_negative() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        sim.queue(Command::Demolish { tile: hq });
        sim.stock = 0;
        sim.queue(Command::Build {
            kind: BuildingKind::Conveyor,
            tile: Tile::new(hq.x - 2, hq.y),
            dir: Dir::East,
        });
        sim.step();
        assert!(sim.buildings.at(hq).is_some());
        assert_eq!(sim.buildings.iter().count(), 1);
    }

    #[test]
    fn undefended_hq_falls_during_the_first_night() {
        let mut sim = Simulation::new(11, 96);
        run(&mut sim, (TICKS_PER_DAY / 2) as u64 + 1);
        assert_eq!(sim.wave, 1);
        // Le QG seul se défend un peu, mais pas assez pour trois nuits.
        run(&mut sim, TICKS_PER_DAY as u64 * 3);
        assert_eq!(sim.outcome, Some(Outcome::Defeat));
        assert!(sim.kills > 0);
    }

    #[test]
    fn same_orders_give_the_same_game() {
        let play = || {
            let mut sim = Simulation::new(21, 96);
            let hq = sim.hq().tile;
            for (i, dir) in Dir::ALL.into_iter().enumerate() {
                sim.queue(Command::Build {
                    kind: BuildingKind::Tower,
                    tile: Tile::new(hq.x - 2 + i as i32 * 2, hq.y - 3),
                    dir,
                });
            }
            run(&mut sim, TICKS_PER_DAY as u64);
            sim.checksum()
        };
        assert_eq!(play(), play());
    }
}
