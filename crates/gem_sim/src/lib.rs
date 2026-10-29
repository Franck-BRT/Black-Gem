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
pub mod gem;
pub mod geom;
pub mod map;
pub mod noise;
pub mod rng;

use std::collections::VecDeque;

pub use building::{BuildError, Building, BuildingKind, BuildingState};
pub use enemy::Enemy;
pub use gem::{Gem, GemColor, Item, TowerProfile};
pub use geom::{Dir, Pos, Tile, UNITS_PER_TILE};
pub use map::{Map, Terrain};

use building::{BuildingState as State, Buildings, HQ_SIZE, Storage};
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
    /// Sertit une gemme de l'inventaire dans la tour de cette case
    /// (l'éventuelle gemme déjà sertie retourne à l'inventaire).
    Socket {
        tile: Tile,
        gem: Gem,
    },
    /// Retire la gemme d'une tour et la remet dans l'inventaire.
    Unsocket {
        tile: Tile,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Defeat,
}

/// Origine d'un tir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotSource {
    Hq,
    Tower,
    /// Rebond d'éclair d'un ennemi à l'autre (gemme jaune).
    Chain,
}

/// Un tir (pour l'affichage des rayons).
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub tick: u64,
    pub from: Pos,
    pub to: Pos,
    pub source: ShotSource,
    /// Couleur de la gemme sertie (aucune : simples éclats de cristal).
    pub color: Option<GemColor>,
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
    /// Gemmes livrées au QG, prêtes à être serties.
    pub gems: Vec<Gem>,
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
        buildings.place(&map, BuildingKind::Hq, hq_tile, Dir::South);

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
            gems: Vec::new(),
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
        self.buildings.update_logistics(&mut Storage {
            crystals: &mut self.stock,
            gems: &mut self.gems,
        });
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
                    self.buildings.place(&self.map, kind, tile, dir);
                }
            }
            Command::Demolish { tile } => {
                if let Some(b) = self.buildings.at(tile)
                    && b.kind != BuildingKind::Hq
                {
                    let id = b.id;
                    let removed = self.buildings.remove(id).unwrap();
                    self.stock += removed.kind.cost() / 2;
                    // La gemme d'une tour démolie n'est pas perdue.
                    if let State::Tower { gem: Some(gem), .. } = removed.state {
                        self.gems.push(gem);
                    }
                }
            }
            Command::Socket { tile, gem } => {
                let Some(id) = self.tower_at(tile) else {
                    return;
                };
                let Some(index) = self.gems.iter().position(|&g| g == gem) else {
                    return;
                };
                self.gems.remove(index);
                if let State::Tower { gem: slot, .. } =
                    &mut self.buildings.get_mut(id).unwrap().state
                    && let Some(old) = slot.replace(gem)
                {
                    self.gems.push(old);
                }
            }
            Command::Unsocket { tile } => {
                if let Some(id) = self.tower_at(tile)
                    && let State::Tower { gem: slot, .. } =
                        &mut self.buildings.get_mut(id).unwrap().state
                    && let Some(old) = slot.take()
                {
                    self.gems.push(old);
                }
            }
        }
    }

    fn tower_at(&self, tile: Tile) -> Option<u32> {
        self.buildings
            .at(tile)
            .filter(|b| b.kind == BuildingKind::Tower)
            .map(|b| b.id)
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
            self.enemies
                .push(Enemy::new(self.next_enemy_id, pos, &spec));
            self.next_enemy_id += 1;
        }
    }

    fn fire_weapons(&mut self) {
        for id in self.buildings.ids() {
            let building = self.buildings.get_mut(id).unwrap();
            let Some((profile, reload)) = building.weapon() else {
                continue;
            };
            let center = building.center();
            let source = if building.kind == BuildingKind::Hq {
                ShotSource::Hq
            } else {
                ShotSource::Tower
            };
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

            // Les cibles les plus proches à portée (plusieurs avec une gemme violette).
            let range_sq = (profile.range as i64).pow(2);
            let mut in_range: Vec<(i64, u32, usize)> = self
                .enemies
                .iter()
                .enumerate()
                .filter(|(_, e)| e.hp > 0 && e.pos.distance_sq(center) <= range_sq)
                .map(|(i, e)| (e.pos.distance_sq(center), e.id, i))
                .collect();
            if in_range.is_empty() {
                continue;
            }
            in_range.sort_unstable();
            *cooldown = reload;
            if let Some(charge) = charge {
                *charge -= 1;
            }

            let color = profile.gem.map(Gem::main_color);
            for &(_, _, target) in in_range.iter().take(1 + profile.extra_targets as usize) {
                hit(&mut self.enemies[target], &profile, profile.damage);
                self.shots.push(Shot {
                    tick: self.tick,
                    from: center,
                    to: self.enemies[target].pos,
                    source,
                    color,
                });
                self.chain_lightning(target, &profile, color);
            }
        }
    }

    /// Éclair jaune : rebondit vers les ennemis voisins, en perdant de la force à chaque saut.
    fn chain_lightning(&mut self, first: usize, profile: &TowerProfile, color: Option<GemColor>) {
        let mut struck = vec![first];
        let mut damage = profile.damage;
        for _ in 0..profile.chain {
            damage = damage * 60 / 100;
            let from = self.enemies[*struck.last().unwrap()].pos;
            let next = self
                .enemies
                .iter()
                .enumerate()
                .filter(|(i, e)| {
                    e.hp > 0
                        && !struck.contains(i)
                        && e.pos.distance_sq(from) <= (gem::CHAIN_RANGE as i64).pow(2)
                })
                .min_by_key(|(_, e)| (e.pos.distance_sq(from), e.id))
                .map(|(i, _)| i);
            let Some(next) = next else {
                break;
            };
            hit(&mut self.enemies[next], profile, damage);
            self.shots.push(Shot {
                tick: self.tick,
                from,
                to: self.enemies[next].pos,
                source: ShotSource::Chain,
                color,
            });
            struck.push(next);
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
        for gem in &self.gems {
            feed(gem.grade as i64);
            for color in gem.colors() {
                feed(color as i64);
            }
        }
        for b in self.buildings.iter() {
            feed(b.id as i64);
            feed(b.hp as i64);
            feed(b.tile.x as i64);
            feed(b.tile.y as i64);
            // Le format `Debug` des états est stable : il sert d'empreinte complète.
            for byte in format!("{:?}", b.state).bytes() {
                feed(byte as i64);
            }
        }
        for e in &self.enemies {
            feed(e.id as i64);
            feed(e.pos.x as i64);
            feed(e.pos.y as i64);
            feed(e.hp as i64);
            feed(e.poison_ticks as i64);
            feed(e.slow_ticks as i64);
        }
        hash
    }
}

/// Applique un coup et les effets de la gemme.
fn hit(enemy: &mut Enemy, profile: &TowerProfile, damage: i32) {
    enemy.hp -= damage;
    if profile.poison_per_second > 0 {
        enemy.poison(profile.poison_per_second);
    }
    if profile.slow_percent > 0 {
        enemy.slow(profile.slow_percent);
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
    fn socketing_swaps_gems_with_the_inventory() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let tile = Tile::new(hq.x - 3, hq.y);
        let red = Gem::new(GemColor::Red, 2);
        let green = Gem::new(GemColor::Green, 1);
        sim.gems = vec![red, green];
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        sim.queue(Command::Socket { tile, gem: red });
        sim.step();
        assert_eq!(sim.gems, vec![green]);
        sim.queue(Command::Socket { tile, gem: green });
        sim.step();
        assert_eq!(sim.gems, vec![red]);
        // Démolir la tour rend sa gemme.
        sim.queue(Command::Demolish { tile });
        sim.step();
        assert_eq!(sim.gems, vec![red, green]);
        // Sertir une gemme absente de l'inventaire ne fait rien.
        sim.queue(Command::Unsocket { tile });
        sim.queue(Command::Socket {
            tile,
            gem: Gem::new(GemColor::Cyan, 5),
        });
        sim.step();
        assert_eq!(sim.gems, vec![red, green]);
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
