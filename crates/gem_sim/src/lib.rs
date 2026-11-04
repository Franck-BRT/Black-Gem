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
pub mod expedition;
pub mod gem;
pub mod geom;
pub mod golem;
pub mod map;
pub mod noise;
pub mod rng;
pub mod unit;

use std::collections::{BTreeMap, VecDeque};

pub use building::{BuildError, Building, BuildingKind, BuildingState};
pub use enemy::{Enemy, Hit};
pub use expedition::{Bonuses, EXTRACTION_TICKS, ExpeditionConfig, Faction, Format};
pub use gem::{Gem, GemColor, Item, TowerProfile};
pub use geom::{Dir, Pos, Tile, UNITS_PER_TILE};
pub use golem::{GOLEM_COST, Golem, GolemStatus, MAX_GOLEMS};
pub use map::{Map, Terrain};
pub use unit::{Ability, Order, Unit, UnitKind};

use building::{BuildingState as State, Buildings, CORE_DRILL_SIZE, HQ_SIZE, Storage};
use enemy::{FlowField, SPAWN_INTERVAL, choose_nests, wave_spec};

/// Fréquence de la simulation (ticks par seconde), fixe quelle que soit la vitesse d'affichage.
pub const TICKS_PER_SECOND: u32 = 30;

/// Durée d'un cycle jour/nuit par défaut, en ticks (6 minutes ; chaque format a la sienne).
pub const TICKS_PER_DAY: u32 = 6 * 60 * TICKS_PER_SECOND;

/// Cristaux disponibles à l'atterrissage.
pub const STARTING_CRYSTALS: u32 = 50;

/// Durée de conservation d'un tir, en ticks (le rendu peut sauter des ticks en vitesse rapide).
const SHOT_LIFETIME: u64 = 30;

/// Ordre donné par un joueur.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Fabrique un golem au QG.
    SpawnGolem,
    /// Compile et lance un programme sur un golem.
    RunProgram {
        golem: u32,
        source: String,
    },
    StopProgram {
        golem: u32,
    },
    /// Mode lent (une instruction à la fois) ou normal.
    SetSlow {
        golem: u32,
        slow: bool,
    },
    /// Commande une unité à la caserne de cette case.
    Train {
        tile: Tile,
    },
    /// Ordre à des unités (elles se placent en formation autour du point visé).
    Order {
        units: Vec<u32>,
        order: Order,
    },
    /// Pouvoir du héros.
    Ability(Ability),
}

/// Ce qui vient de se passer pendant un tick, pour le son, les effets et les alertes. Ce
/// n'est pas de l'état : rien n'en dépend dans la simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimEvent {
    Built {
        kind: BuildingKind,
        tile: Tile,
    },
    Demolished {
        tile: Tile,
    },
    Shot {
        color: Option<GemColor>,
    },
    EnemyKilled {
        pos: Pos,
        boss: bool,
    },
    HqHit,
    BuildingHit {
        kind: BuildingKind,
        tile: Tile,
    },
    BuildingDestroyed {
        kind: BuildingKind,
        tile: Tile,
    },
    /// La nuit tombe : une vague arrive.
    NightFalls {
        wave: u32,
    },
    /// Un palier de forage est franchi : des boss arrivent.
    StageReached {
        stage: u32,
    },
    GolemBuilt,
    Victory,
    Defeat,
    UnitTrained,
    UnitLost,
    NestDestroyed {
        nest: usize,
    },
    HeroLevelUp {
        level: u32,
    },
    AbilityUsed {
        ability: Ability,
        pos: Pos,
    },
    /// L'énergie vient à manquer (les machines ralentissent).
    PowerShortage,
}

/// Points de vie d'un nid (avant la difficulté).
const NEST_HP: i32 = 2500;
/// Cristaux gagnés en détruisant un nid.
pub const NEST_REWARD: u32 = 60;
/// Ticks entre deux mises à jour du brouillard de guerre.
const FOG_PERIOD: u64 = 10;

/// Délai sans être frappé avant que la réparation automatique commence.
const REPAIR_DELAY: u64 = 10 * TICKS_PER_SECOND as u64;

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
    /// Tir d'une unité de combat.
    Unit,
}

/// Décalage (en cases) de la k-ième unité d'un groupe : en carré autour du point visé.
fn formation_offset(k: usize) -> (i32, i32) {
    let side = 5;
    let (col, row) = ((k % side) as i32, (k / side) as i32);
    (col - side as i32 / 2, row)
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

/// Un ennemi qui va sortir d'un nid.
#[derive(Clone, Copy, Debug)]
struct PendingSpawn {
    at: u64,
    nest: usize,
    /// Force de la vague (voir `wave_spec`).
    level: u32,
    boss: bool,
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
    pub golems: Vec<Golem>,
    pub nests: Vec<Tile>,
    pub config: ExpeditionConfig,
    /// Le Cœur : coin nord-ouest du carré où construire le Grand foreur (absent en Colonie).
    pub core: Option<Tile>,
    /// Cristaux brûlés par le Grand foreur.
    pub drilled: u32,
    /// Paliers de forage franchis.
    pub stage: u32,
    /// Fin du décollage, une fois la Gemme Noire extraite.
    pub extraction_until: Option<u64>,
    /// Nombre de nuits (vagues nocturnes) déjà lancées.
    pub wave: u32,
    pub kills: u32,
    pub shots: Vec<Shot>,
    pub outcome: Option<Outcome>,
    /// Événements du dernier tick.
    pub events: Vec<SimEvent>,
    /// Cristaux livrés au QG (par convoyeur ou par golem) depuis le début.
    pub delivered: u32,
    /// Coordination des golems : cases réservées (par quel golem)…
    pub reservations: BTreeMap<Tile, u32>,
    /// … et canaux de messages (numéro → messages en attente).
    pub channels: BTreeMap<i64, VecDeque<i64>>,
    /// Unités de combat (dont le héros).
    pub units: Vec<Unit>,
    /// Points de vie des nids (0 : détruit).
    pub nest_hp: Vec<i32>,
    pub nest_max_hp: i32,
    /// Cases déjà vues (brouillard de guerre), ligne par ligne.
    pub explored: Vec<bool>,
    /// Énergie produite et consommée.
    pub power: (i32, i32),
    /// Héros tombé : il revient au QG à ce tick (son niveau est gardé).
    pub fallen_hero: Option<(Unit, u64)>,
    /// Tick à partir duquel chaque pouvoir du héros est de nouveau prêt.
    pub ability_ready: [u64; 2],
    power_acc: u32,
    next_unit_id: u32,
    field: FlowField,
    /// Apparitions prévues, dans l'ordre chronologique.
    pending_spawns: VecDeque<PendingSpawn>,
    commands: Vec<Command>,
    next_enemy_id: u32,
}

impl Simulation {
    /// Expédition standard sur une carte de la taille donnée (utile pour les tests).
    pub fn new(seed: u64, map_size: u32) -> Self {
        Self::with_config(
            seed,
            ExpeditionConfig {
                map_size,
                ticks_per_day: TICKS_PER_DAY,
                ..Format::Expedition.config()
            },
        )
    }

    pub fn with_format(seed: u64, format: Format) -> Self {
        Self::with_config(seed, format.config())
    }

    pub fn with_config(seed: u64, config: ExpeditionConfig) -> Self {
        let map_size = config.map_size;
        let map = Map::generate(seed, map_size);
        let mut buildings = Buildings::new(map_size, config.bonuses);
        let corner = (map_size as i32 - HQ_SIZE) / 2;
        let hq_tile = Tile::new(corner, corner);
        buildings.place(&map, BuildingKind::Hq, hq_tile, Dir::South);

        let hq_tiles: Vec<Tile> = (0..HQ_SIZE)
            .flat_map(|dy| (0..HQ_SIZE).map(move |dx| Tile::new(corner + dx, corner + dy)))
            .collect();
        let field = FlowField::toward(&map, &hq_tiles);
        let nests = choose_nests(&map, &field);
        let nests_len = nests.len();
        let nest_hp = NEST_HP * config.difficulty as i32 / 100;
        let core = (config.stages > 0)
            .then(|| choose_core(&map, &field, hq_tile))
            .flatten();

        let mut sim = Self {
            map,
            // On atterrit en début de matinée.
            tick: (config.ticks_per_day / 4) as u64,
            buildings,
            enemies: Vec::new(),
            stock: config.bonuses.starting_crystals,
            // Recherche « gemme de départ » : une gemme de grade 2, de couleur tirée de la graine.
            gems: if config.bonuses.starting_gem {
                vec![Gem::new(GemColor::ALL[(seed % 5) as usize], 2)]
            } else {
                Vec::new()
            },
            golems: Vec::new(),
            nests,
            config,
            core,
            drilled: 0,
            stage: 0,
            extraction_until: None,
            wave: 0,
            kills: 0,
            shots: Vec::new(),
            events: Vec::new(),
            delivered: 0,
            reservations: BTreeMap::new(),
            channels: BTreeMap::new(),
            units: Vec::new(),
            nest_hp: vec![nest_hp; nests_len],
            nest_max_hp: nest_hp,
            explored: vec![false; (map_size * map_size) as usize],
            power: (0, 0),
            fallen_hero: None,
            ability_ready: [0; 2],
            power_acc: 0,
            next_unit_id: 1,
            outcome: None,
            field,
            pending_spawns: VecDeque::new(),
            commands: Vec::new(),
            next_enemy_id: 1,
        };
        // Le Forgeron-Ingénieur débarque avec le QG.
        let mut pos = sim.hq().center();
        pos.y += 3 * UNITS_PER_TILE;
        pos.x += 2 * UNITS_PER_TILE;
        let hero = Unit::new(sim.next_unit_id, UnitKind::Hero, pos);
        sim.next_unit_id += 1;
        sim.units.push(hero);
        sim.explore();
        // Repérés depuis l'orbite : le Cœur et les nids (leur lueur se voit de loin).
        let mut landmarks: Vec<(Pos, i32)> = sim.nests.iter().map(|n| (n.center(), 3)).collect();
        if let Some(core) = sim.core {
            landmarks.push((Tile::new(core.x + 1, core.y + 1).center(), 4));
        }
        sim.reveal(&landmarks);
        sim
    }

    /// Avancement du forage, en pour cent (0 sans Cœur).
    pub fn drilling_percent(&self) -> u32 {
        match self.config.total_crystals() {
            0 => 0,
            total => self.drilled.min(total) * 100 / total,
        }
    }

    /// Le Grand foreur est-il construit ?
    pub fn core_drill(&self) -> Option<&Building> {
        self.buildings
            .iter()
            .find(|b| b.kind == BuildingKind::CoreDrill)
    }

    /// Heure du jour, en millièmes de journée : 0 = minuit, 250 = lever, 500 = midi, 750 = coucher.
    pub fn time_of_day(&self) -> u32 {
        let day = self.config.ticks_per_day as u64;
        ((self.tick % day) * 1000 / day) as u32
    }

    /// La nuit, la Corruption attaque.
    pub fn is_night(&self) -> bool {
        !(250..750).contains(&self.time_of_day())
    }

    /// Ticks avant la prochaine tombée de la nuit.
    pub fn ticks_until_nightfall(&self) -> u64 {
        let day = self.config.ticks_per_day as u64;
        let dusk = day * 3 / 4;
        (dusk + day - self.tick % day) % day
    }

    pub fn hq(&self) -> &Building {
        self.buildings.hq().expect("le QG existe toujours")
    }

    /// Une construction serait-elle acceptée ?
    pub fn check_build(&self, kind: BuildingKind, tile: Tile) -> Result<(), BuildError> {
        match (kind, self.core) {
            (BuildingKind::CoreDrill, None) => return Err(BuildError::NoCore),
            (BuildingKind::CoreDrill, Some(core)) if core != tile => {
                return Err(BuildError::NeedsCore);
            }
            (BuildingKind::CoreDrill, _) => {}
            (_, Some(core)) => {
                let inside = |t: Tile| {
                    (core.x..core.x + CORE_DRILL_SIZE).contains(&t.x)
                        && (core.y..core.y + CORE_DRILL_SIZE).contains(&t.y)
                };
                let size = kind.size();
                let overlaps = (0..size)
                    .flat_map(|dy| (0..size).map(move |dx| Tile::new(tile.x + dx, tile.y + dy)))
                    .any(inside);
                if overlaps {
                    return Err(BuildError::CoreSite);
                }
            }
            (_, None) => {}
        }
        self.buildings.check_site(&self.map, kind, tile)?;
        let size = kind.size();
        let unexplored = (0..size)
            .flat_map(|dy| (0..size).map(move |dx| Tile::new(tile.x + dx, tile.y + dy)))
            .any(|t| !self.is_explored(t));
        if unexplored {
            return Err(BuildError::Unexplored);
        }
        if let Some(required) = kind.requires()
            && !self.buildings.iter().any(|b| b.kind == required)
        {
            return Err(BuildError::Requires(required));
        }
        if self.stock < kind.cost() {
            return Err(BuildError::NotEnoughCrystals);
        }
        Ok(())
    }

    /// La case a-t-elle déjà été vue ?
    pub fn is_explored(&self, tile: Tile) -> bool {
        let size = self.map.size as i32;
        (0..size).contains(&tile.x)
            && (0..size).contains(&tile.y)
            && self.explored[(tile.y * size + tile.x) as usize]
    }

    /// Ce qui voit (pour le brouillard de guerre) : position et rayon, en cases.
    pub fn vision_sources(&self) -> Vec<(Pos, i32)> {
        let mut sources: Vec<(Pos, i32)> = self
            .buildings
            .iter()
            .map(|b| (b.center(), b.kind.sight()))
            .collect();
        sources.extend(self.units.iter().map(|u| (u.pos, u.kind.stats().sight)));
        sources.extend(self.golems.iter().map(|g| (g.pos, 5)));
        sources
    }

    /// Découvre la carte autour de tout ce qui voit.
    fn explore(&mut self) {
        let sources = self.vision_sources();
        self.reveal(&sources);
    }

    fn reveal(&mut self, sources: &[(Pos, i32)]) {
        let size = self.map.size as i32;
        for &(pos, radius) in sources {
            let center = pos.tile();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let (x, y) = (center.x + dx, center.y + dy);
                    if (0..size).contains(&x) && (0..size).contains(&y) {
                        self.explored[(y * size + x) as usize] = true;
                    }
                }
            }
        }
    }

    /// Le type d'unité que forme la caserne (selon la faction).
    pub fn trained_kind(&self) -> UnitKind {
        match self.config.faction {
            Faction::Smiths => UnitKind::Sentinel,
            Faction::Settlers => UnitKind::Tank,
        }
    }

    /// Ravitaillement : (utilisé, maximum). Les unités commandées comptent déjà.
    pub fn supply(&self) -> (u32, u32) {
        let habitats = self
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Habitat)
            .count() as u32;
        let queued: u32 = self
            .buildings
            .iter()
            .map(|b| match b.state {
                State::Barracks { queue, .. } => queue,
                _ => 0,
            })
            .sum();
        let used = self
            .units
            .iter()
            .map(|u| u.kind.stats().supply)
            .sum::<u32>()
            + queued * self.trained_kind().stats().supply;
        (
            used,
            building::HQ_SUPPLY + building::HABITAT_SUPPLY * habitats,
        )
    }

    /// Le héros (sur le terrain).
    pub fn hero(&self) -> Option<&Unit> {
        self.units.iter().find(|u| u.kind == UnitKind::Hero)
    }

    /// Enregistre un ordre ; il sera exécuté au début du prochain tick.
    pub fn queue(&mut self, command: Command) {
        self.commands.push(command);
    }

    /// Avance la simulation d'un tick.
    pub fn step(&mut self) {
        self.events.clear();
        if self.outcome.is_some() {
            return;
        }
        for command in std::mem::take(&mut self.commands) {
            self.apply(command);
        }
        self.update_waves();
        self.update_power();
        let before = self.stock;
        self.buildings.update_logistics(&mut Storage {
            crystals: &mut self.stock,
            gems: &mut self.gems,
            drilled: &mut self.drilled,
        });
        self.delivered += self.stock.saturating_sub(before);
        let before = self.stock;
        self.update_golems();
        self.delivered += self.stock.saturating_sub(before);
        self.repair();
        self.update_drilling();
        self.train_units();
        self.update_units();
        self.fire_weapons();
        self.update_enemies();
        if self.tick.is_multiple_of(FOG_PERIOD) {
            self.explore();
        }
        self.shots.retain(|s| s.tick + SHOT_LIFETIME > self.tick);
        self.tick += 1;
    }

    /// Bilan énergétique. En manque, les machines n'avancent qu'une partie des ticks
    /// (à 50 % d'énergie, un tick sur deux) : tours, foreuses et ateliers ralentissent.
    fn update_power(&mut self) {
        let settlers = self.config.faction == Faction::Settlers;
        let (mut produced, mut used) = (0, 0);
        for b in self.buildings.iter() {
            let mut power = b.kind.power();
            if settlers && b.kind == BuildingKind::PowerPlant {
                power = power * 3 / 2;
            }
            if power > 0 {
                produced += power;
            } else {
                used -= power;
            }
        }
        let was_short = self.power.0 < self.power.1;
        self.power = (produced, used);
        if produced < used && !was_short {
            self.events.push(SimEvent::PowerShortage);
        }
        let percent = if used == 0 {
            100
        } else {
            (produced * 100 / used).clamp(0, 100) as u32
        };
        self.power_acc += percent;
        self.buildings.powered = self.power_acc >= 100;
        if self.buildings.powered {
            self.power_acc -= 100;
        }
    }

    /// Les casernes forment les unités commandées.
    fn train_units(&mut self) {
        let kind = self.trained_kind();
        let ticks = kind.stats().train_ticks;
        let powered = self.buildings.powered;
        for id in self.buildings.ids() {
            let building = self.buildings.get_mut(id).unwrap();
            let center = building.center();
            let State::Barracks { queue, progress } = &mut building.state else {
                continue;
            };
            if *queue == 0 || !powered {
                continue;
            }
            *progress += 1;
            if *progress < ticks {
                continue;
            }
            *progress = 0;
            *queue -= 1;
            let mut pos = center;
            pos.y += UNITS_PER_TILE + (self.next_unit_id as i32 % 3) * UNITS_PER_TILE / 2;
            pos.x += (self.next_unit_id as i32 % 5 - 2) * UNITS_PER_TILE / 2;
            self.units.push(Unit::new(self.next_unit_id, kind, pos));
            self.next_unit_id += 1;
            self.events.push(SimEvent::UnitTrained);
        }
    }

    /// Les unités bougent et frappent ; le héros revient s'il est tombé.
    fn update_units(&mut self) {
        if let Some((_, at)) = &self.fallen_hero
            && self.tick >= *at
        {
            let (mut hero, _) = self.fallen_hero.take().unwrap();
            hero.hp = hero.max_hp;
            hero.order = Order::Idle;
            let mut pos = self.hq().center();
            pos.y += 3 * UNITS_PER_TILE;
            hero.pos = pos;
            hero.prev_pos = pos;
            self.units.push(hero);
        }
        let enemies: Vec<(u32, Pos)> = self
            .enemies
            .iter()
            .filter(|e| e.hp > 0)
            .map(|e| (e.id, e.pos))
            .collect();
        let nests: Vec<(usize, Pos)> = self
            .nests
            .iter()
            .enumerate()
            .filter(|(i, _)| self.nest_hp[*i] > 0)
            .map(|(i, t)| (i, t.center()))
            .collect();
        let around = unit::Surroundings {
            enemies: &enemies,
            nests: &nests,
        };
        let strikes: Vec<(usize, unit::Strike)> = self
            .units
            .iter_mut()
            .enumerate()
            .map(|(i, u)| (i, u.update(&around)))
            .collect();
        for (i, strike) in strikes {
            match strike {
                unit::Strike::Nothing => {}
                unit::Strike::Enemy(id, damage) => {
                    let from = self.units[i].pos;
                    let Some(enemy) = self.enemies.iter_mut().find(|e| e.id == id) else {
                        continue;
                    };
                    let alive = enemy.hp > 0;
                    enemy.hp -= damage;
                    let to = enemy.pos;
                    let killed = alive && enemy.hp <= 0;
                    let xp = if enemy.boss { 60 } else { 12 };
                    self.shots.push(Shot {
                        tick: self.tick,
                        from,
                        to,
                        source: ShotSource::Unit,
                        color: None,
                    });
                    if killed && self.units[i].gain_xp(xp) {
                        let level = self.units[i].level;
                        self.events.push(SimEvent::HeroLevelUp { level });
                    }
                }
                unit::Strike::Nest(nest, damage) => {
                    let from = self.units[i].pos;
                    self.shots.push(Shot {
                        tick: self.tick,
                        from,
                        to: self.nests[nest].center(),
                        source: ShotSource::Unit,
                        color: None,
                    });
                    self.damage_nest(nest, damage);
                    if self.nest_hp[nest] <= 0 && self.units[i].gain_xp(150) {
                        let level = self.units[i].level;
                        self.events.push(SimEvent::HeroLevelUp { level });
                    }
                }
            }
        }
    }

    fn damage_nest(&mut self, nest: usize, damage: i32) {
        if self.nest_hp[nest] <= 0 {
            return;
        }
        self.nest_hp[nest] -= damage;
        if self.nest_hp[nest] > 0 {
            return;
        }
        self.nest_hp[nest] = 0;
        self.stock += NEST_REWARD;
        self.pending_spawns.retain(|s| s.nest != nest);
        self.events.push(SimEvent::NestDestroyed { nest });
        // En Colonie (pas de Cœur), détruire tous les nids est la victoire.
        if self.core.is_none() && self.nest_hp.iter().all(|&hp| hp <= 0) && self.outcome.is_none() {
            self.outcome = Some(Outcome::Victory);
            self.events.push(SimEvent::Victory);
        }
    }

    /// Pouvoir du héros.
    fn use_ability(&mut self, ability: Ability) {
        let Some(hero) = self.hero() else {
            return;
        };
        let (pos, level) = (hero.pos, hero.level);
        if level < ability.min_level() || self.tick < self.ability_ready[ability.index()] {
            return;
        }
        self.ability_ready[ability.index()] = self.tick + ability.cooldown();
        let radius_sq = (ability.radius() as i64).pow(2);
        let damage = ability.damage(level);
        let mut xp = 0;
        for enemy in &mut self.enemies {
            if enemy.hp <= 0 || enemy.pos.distance_sq(pos) > radius_sq {
                continue;
            }
            enemy.hp -= damage;
            if ability == Ability::FrostWave {
                enemy.slow(60);
            }
            if enemy.hp <= 0 {
                xp += if enemy.boss { 60 } else { 12 };
            }
        }
        if let Some(hero) = self.units.iter_mut().find(|u| u.kind == UnitKind::Hero)
            && hero.gain_xp(xp)
        {
            let level = hero.level;
            self.events.push(SimEvent::HeroLevelUp { level });
        }
        self.events.push(SimEvent::AbilityUsed { ability, pos });
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Build { kind, tile, dir } => {
                if kind != BuildingKind::Hq && self.check_build(kind, tile).is_ok() {
                    self.stock -= kind.cost();
                    self.buildings.place(&self.map, kind, tile, dir);
                    self.events.push(SimEvent::Built { kind, tile });
                }
            }
            Command::Demolish { tile } => {
                if let Some(b) = self.buildings.at(tile)
                    && b.kind != BuildingKind::Hq
                {
                    let id = b.id;
                    let removed = self.buildings.remove(id).unwrap();
                    self.stock += removed.kind.cost() / 2;
                    self.events.push(SimEvent::Demolished { tile });
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
            Command::SpawnGolem => {
                let cost = self.config.bonuses.golem_cost;
                if self.stock >= cost && self.golems.len() < MAX_GOLEMS {
                    self.stock -= cost;
                    let id = self.golems.iter().map(|g| g.id).max().unwrap_or(0) + 1;
                    let mut pos = self.hq().center();
                    // Les golems sortent du QG côté sud, l'un à côté de l'autre.
                    pos.y += 2 * UNITS_PER_TILE;
                    pos.x += (id as i32 % 4 - 2) * UNITS_PER_TILE;
                    self.golems.push(Golem::new(id, pos));
                    self.events.push(SimEvent::GolemBuilt);
                }
            }
            Command::RunProgram { golem, source } => {
                if let Some(golem) = self.golem_mut(golem) {
                    golem.start(source);
                }
            }
            Command::StopProgram { golem } => {
                if let Some(golem) = self.golem_mut(golem) {
                    golem.stop();
                }
            }
            Command::SetSlow { golem, slow } => {
                if let Some(golem) = self.golem_mut(golem) {
                    golem.slow = slow;
                }
            }
            Command::Train { tile } => {
                let kind = self.trained_kind();
                let stats = kind.stats();
                let (used, cap) = self.supply();
                if self.stock < stats.cost || used + stats.supply > cap {
                    return;
                }
                let Some(id) = self.buildings.at(tile).map(|b| b.id) else {
                    return;
                };
                if let State::Barracks { queue, .. } =
                    &mut self.buildings.get_mut(id).unwrap().state
                    && *queue < building::BARRACKS_QUEUE
                {
                    *queue += 1;
                    self.stock -= stats.cost;
                }
            }
            Command::Order { mut units, order } => {
                units.sort_unstable();
                units.dedup();
                for (k, id) in units.into_iter().enumerate() {
                    let Some(unit) = self.units.iter_mut().find(|u| u.id == id) else {
                        continue;
                    };
                    let (dx, dy) = formation_offset(k);
                    let shift =
                        |p: Pos| Pos::new(p.x + dx * UNITS_PER_TILE, p.y + dy * UNITS_PER_TILE);
                    unit.order = match order {
                        Order::Idle => Order::Idle,
                        Order::Move(p) => Order::Move(shift(p)),
                        Order::AttackMove(p) => Order::AttackMove(shift(p)),
                    };
                }
            }
            Command::Ability(ability) => self.use_ability(ability),
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

    fn update_golems(&mut self) {
        let core = self.core.map(|c| Tile::new(c.x + 1, c.y + 1));
        let drilling_percent = self.drilling_percent();
        let mut world = golem::World {
            buildings: &mut self.buildings,
            storage: Storage {
                crystals: &mut self.stock,
                gems: &mut self.gems,
                drilled: &mut self.drilled,
            },
            core,
            drilling_percent,
            golem_speed: self.config.bonuses.golem_speed,
            enemies: &self.enemies,
            map_size: self.map.size,
            tick: self.tick,
            reservations: &mut self.reservations,
            channels: &mut self.channels,
            golem_count: self.golems.len() as u32,
        };
        for golem in &mut self.golems {
            golem.update(&mut world);
        }
        // Un golem arrêté (fin, erreur) libère ses réservations.
        let running: Vec<u32> = self
            .golems
            .iter()
            .filter(|g| g.status == GolemStatus::Running)
            .map(|g| g.id)
            .collect();
        self.reservations.retain(|_, owner| running.contains(owner));
    }

    pub fn golem(&self, id: u32) -> Option<&Golem> {
        self.golems.iter().find(|g| g.id == id)
    }

    fn golem_mut(&mut self, id: u32) -> Option<&mut Golem> {
        self.golems.iter_mut().find(|g| g.id == id)
    }

    fn tower_at(&self, tile: Tile) -> Option<u32> {
        self.buildings
            .at(tile)
            .filter(|b| b.kind == BuildingKind::Tower)
            .map(|b| b.id)
    }

    /// Programme une vague : `per_nest` ennemis de force `level` à chaque nid (et un boss par nid).
    fn schedule_wave(&mut self, level: u32, bosses: bool) {
        let spec = wave_spec(level, self.config.difficulty);
        for nest in 0..self.nests.len() {
            if bosses && self.nest_hp[nest] > 0 {
                self.pending_spawns.push_back(PendingSpawn {
                    at: self.tick + nest as u64 * 7,
                    nest,
                    level,
                    boss: true,
                });
            }
        }
        for i in 0..spec.per_nest as u64 {
            for nest in (0..self.nests.len()).filter(|&n| self.nest_hp[n] > 0) {
                self.pending_spawns.push_back(PendingSpawn {
                    at: self.tick + (i + bosses as u64) * SPAWN_INTERVAL + nest as u64 * 7,
                    nest,
                    level,
                    boss: false,
                });
            }
        }
    }

    fn update_waves(&mut self) {
        let day = self.config.ticks_per_day as u64;
        // Chaque nuit, une vague plus forte que la précédente.
        if self.tick % day == day * 3 / 4 && self.extraction_until.is_none() {
            self.wave += 1;
            self.schedule_wave(self.wave, false);
            self.events.push(SimEvent::NightFalls { wave: self.wave });
        }
        while let Some(&spawn) = self.pending_spawns.front() {
            if spawn.at > self.tick {
                break;
            }
            self.pending_spawns.pop_front();
            let spec = wave_spec(spawn.level, self.config.difficulty);
            let pos = self.nests[spawn.nest].center();
            let enemy = if spawn.boss {
                Enemy::boss(self.next_enemy_id, pos, &spec)
            } else {
                Enemy::new(self.next_enemy_id, pos, &spec)
            };
            self.enemies.push(enemy);
            self.next_enemy_id += 1;
        }
    }

    /// Paliers de forage : chacun déclenche une vague majeure avec des boss ; le dernier
    /// extrait la Gemme Noire et lance le décollage.
    fn update_drilling(&mut self) {
        let stages = self.config.stages;
        if stages == 0 {
            return;
        }
        let reached = (self.drilled / self.config.crystals_per_stage).min(stages);
        while self.stage < reached {
            self.stage += 1;
            self.events
                .push(SimEvent::StageReached { stage: self.stage });
            let level = self.wave + self.stage + 1;
            self.schedule_wave(level, true);
            if self.stage == stages {
                self.extraction_until = Some(self.tick + EXTRACTION_TICKS);
            }
        }
    }

    fn fire_weapons(&mut self) {
        let powered = self.buildings.powered;
        for id in self.buildings.ids() {
            let building = self.buildings.get_mut(id).unwrap();
            let Some((mut profile, reload)) = building.weapon() else {
                continue;
            };
            if building.kind == BuildingKind::Tower {
                let bonuses = &self.config.bonuses;
                profile.damage = profile.damage * bonuses.tower_damage / 100;
                if self.config.faction == Faction::Smiths {
                    profile.damage = profile.damage * 115 / 100;
                }
                profile.range += bonuses.tower_range;
            }
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
                // Sans énergie, les tours rechargent plus lentement (le QG a la sienne).
                if charge.is_none() || powered {
                    *cooldown -= 1;
                }
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
            self.events.push(SimEvent::Shot { color });
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
        for e in self.enemies.iter().filter(|e| e.hp <= 0) {
            self.events.push(SimEvent::EnemyKilled {
                pos: e.pos,
                boss: e.boss,
            });
        }
        let before = self.enemies.len();
        self.enemies.retain(|e| e.hp > 0);
        self.kills += (before - self.enemies.len()) as u32;

        let hq_center = self.hq().center();
        let buildings = &self.buildings;
        let blocked = |tile: Tile| {
            buildings
                .at(tile)
                .is_some_and(|b| b.kind != BuildingKind::Hq)
        };
        let units: Vec<(u32, Pos)> = self.units.iter().map(|u| (u.id, u.pos)).collect();
        let contact = (UNITS_PER_TILE as i64 * 3 / 2).pow(2);
        let unit_near = |p: Pos| {
            units
                .iter()
                .filter(|(_, q)| q.distance_sq(p) <= contact)
                .min_by_key(|(id, q)| (q.distance_sq(p), *id))
                .map(|(id, _)| *id)
        };
        let hits: Vec<Hit> = self
            .enemies
            .iter_mut()
            .map(|e| e.update(&self.field, hq_center, blocked, unit_near))
            .collect();
        let mut hq_damage = 0;
        for hit in hits {
            match hit {
                Hit::Nothing => {}
                Hit::Hq(damage) => hq_damage += damage,
                Hit::Building(tile, damage) => self.damage_building(tile, damage),
                Hit::Unit(id, damage) => {
                    if let Some(unit) = self.units.iter_mut().find(|u| u.id == id) {
                        unit.hp -= damage;
                    }
                }
            }
        }
        // Unités tombées (le héros reviendra au QG).
        let mut i = 0;
        while i < self.units.len() {
            if self.units[i].hp > 0 {
                i += 1;
                continue;
            }
            let unit = self.units.remove(i);
            self.events.push(SimEvent::UnitLost);
            if unit.kind == UnitKind::Hero {
                self.fallen_hero = Some((unit, self.tick + unit::HERO_RESPAWN_TICKS));
            }
        }
        if hq_damage > 0 {
            let hq_id = self.hq().id;
            let tick = self.tick;
            let hq = self.buildings.get_mut(hq_id).unwrap();
            hq.hp -= hq_damage;
            hq.last_hit = tick;
            self.events.push(SimEvent::HqHit);
            if hq.hp <= 0 {
                self.outcome = Some(Outcome::Defeat);
                self.events.push(SimEvent::Defeat);
            }
        }
        if self
            .extraction_until
            .is_some_and(|until| self.tick >= until)
            && self.outcome.is_none()
        {
            self.outcome = Some(Outcome::Victory);
            self.events.push(SimEvent::Victory);
        }
    }

    /// Un ennemi frappe un bâtiment ; à zéro, le bâtiment est détruit (la gemme d'une tour
    /// revient dans l'inventaire : elle est indestructible).
    fn damage_building(&mut self, tile: Tile, damage: i32) {
        let Some(id) = self.buildings.at(tile).map(|b| b.id) else {
            return;
        };
        let tick = self.tick;
        let building = self.buildings.get_mut(id).unwrap();
        building.hp -= damage;
        building.last_hit = tick;
        let (kind, origin) = (building.kind, building.tile);
        if building.hp > 0 {
            self.events
                .push(SimEvent::BuildingHit { kind, tile: origin });
            return;
        }
        let removed = self.buildings.remove(id).unwrap();
        if let State::Tower { gem: Some(gem), .. } = removed.state {
            self.gems.push(gem);
        }
        self.events
            .push(SimEvent::BuildingDestroyed { kind, tile: origin });
    }

    /// Réparation automatique : un bâtiment qui n'est plus attaqué depuis un moment récupère
    /// 2 % de sa vie par seconde.
    fn repair(&mut self) {
        if !self.tick.is_multiple_of(TICKS_PER_SECOND as u64) {
            return;
        }
        let tick = self.tick;
        for id in self.buildings.ids() {
            let building = self.buildings.get_mut(id).unwrap();
            if building.hp < building.max_hp && tick >= building.last_hit + REPAIR_DELAY {
                building.hp = (building.hp + (building.max_hp / 50).max(1)).min(building.max_hp);
            }
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
            feed(b.last_hit as i64);
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
        for g in &self.golems {
            feed(g.id as i64);
            feed(g.pos.x as i64);
            feed(g.pos.y as i64);
            feed(g.line.unwrap_or(0) as i64);
            feed(g.slow as i64);
            for byte in format!("{:?}{:?}", g.carrying, g.status).bytes() {
                feed(byte as i64);
            }
        }
        for u in &self.units {
            feed(u.id as i64);
            feed(u.pos.x as i64);
            feed(u.pos.y as i64);
            feed(u.hp as i64);
            feed(u.xp as i64);
            for byte in format!("{:?}", u.order).bytes() {
                feed(byte as i64);
            }
        }
        for hp in &self.nest_hp {
            feed(*hp as i64);
        }
        feed(self.explored.iter().filter(|e| **e).count() as i64);
        feed(self.power_acc as i64);
        for (tile, owner) in &self.reservations {
            feed(tile.x as i64);
            feed(tile.y as i64);
            feed(*owner as i64);
        }
        for (channel, messages) in &self.channels {
            feed(*channel);
            for m in messages {
                feed(*m);
            }
        }
        hash
    }
}

/// Place le Cœur : un carré constructible, ni trop près ni trop loin du QG, atteignable par
/// la Corruption (il faudra le défendre), et choisi « au hasard » d'après la graine.
fn choose_core(map: &Map, field: &FlowField, hq: Tile) -> Option<Tile> {
    let size = map.size as i32;
    let (near, far) = ((size / 7).max(14), (size / 4).max(20));
    let hq_center = Tile::new(hq.x + HQ_SIZE / 2, hq.y + HQ_SIZE / 2);
    let site_ok = |t: Tile| {
        (0..CORE_DRILL_SIZE).all(|dy| {
            (0..CORE_DRILL_SIZE).all(|dx| {
                let (x, y) = (t.x + dx, t.y + dy);
                (0..size).contains(&x)
                    && (0..size).contains(&y)
                    && matches!(map.tile(x as u32, y as u32), Terrain::Plain | Terrain::Rock)
            })
        }) && field.distance(Tile::new(t.x + 1, t.y + 1)).is_some()
    };
    (0..size)
        .flat_map(|y| (0..size).map(move |x| Tile::new(x, y)))
        .filter(|t| {
            let d2 = (t.x + 1 - hq_center.x).pow(2) + (t.y + 1 - hq_center.y).pow(2);
            (near * near..=far * far).contains(&d2)
        })
        .filter(|t| site_ok(*t))
        .min_by_key(|t| (rng::hash2(map.seed ^ 0xC0E0, t.x, t.y), *t))
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
    fn enemies_destroy_buildings_in_their_way_and_gems_survive() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let tile = Tile::new(hq.x - 6, hq.y + 1);
        sim.stock = 500;
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        sim.step();
        assert_eq!(
            sim.events,
            vec![SimEvent::Built {
                kind: BuildingKind::Tower,
                tile
            }]
        );
        let gem = Gem::new(GemColor::Red, 1);
        if let State::Tower { gem: slot, .. } = &mut sim
            .buildings
            .get_mut(sim.buildings.at(tile).unwrap().id)
            .unwrap()
            .state
        {
            *slot = Some(gem);
        }
        // Un ennemi très résistant, posé sur la tour (qui ne peut pas tirer sans munitions).
        let spec = wave_spec(1, 100);
        let mut enemy = Enemy::new(999, tile.center(), &spec);
        enemy.hp = i32::MAX / 2;
        sim.enemies.push(enemy);
        let mut hit = false;
        let mut destroyed = false;
        for _ in 0..30 * 40 {
            sim.step();
            hit |= sim
                .events
                .iter()
                .any(|e| matches!(e, SimEvent::BuildingHit { .. }));
            destroyed |= sim.events.contains(&SimEvent::BuildingDestroyed {
                kind: BuildingKind::Tower,
                tile,
            });
            if destroyed {
                break;
            }
        }
        assert!(hit && destroyed);
        assert!(sim.buildings.at(tile).is_none());
        assert_eq!(sim.gems, vec![gem]);
    }

    #[test]
    fn damaged_buildings_repair_themselves_after_a_while() {
        let mut sim = Simulation::new(4, 96);
        let id = sim.hq().id;
        let tick = sim.tick;
        let hq = sim.buildings.get_mut(id).unwrap();
        hq.hp -= 100;
        hq.last_hit = tick;
        run(&mut sim, 5 * TICKS_PER_SECOND as u64);
        assert_eq!(sim.hq().hp, sim.hq().max_hp - 100);
        run(&mut sim, 10 * TICKS_PER_SECOND as u64);
        assert!(sim.hq().hp > sim.hq().max_hp - 100);
        assert!(sim.hq().hp <= sim.hq().max_hp);
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
    fn a_golem_program_supplies_a_tower() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let tile = Tile::new(hq.x - 4, hq.y + 1);
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        sim.queue(Command::SpawnGolem);
        sim.step();
        let stock = sim.stock;
        sim.queue(Command::RunProgram {
            golem: 1,
            source: format!(
                "aller_vers(qg())\nprendre()\naller_vers({}, {})\ndeposer()\ndire('fini', munitions({}, {}))",
                tile.x, tile.y, tile.x, tile.y
            ),
        });
        run(&mut sim, 200);
        let golem = sim.golem(1).unwrap();
        assert_eq!(golem.status, GolemStatus::Finished, "{:?}", golem.status);
        assert_eq!(golem.log, vec!["fini 6"]);
        assert_eq!(sim.stock, stock - 1);
    }

    #[test]
    fn a_rust_golem_moves_its_item_into_a_tower() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let tile = Tile::new(hq.x - 4, hq.y + 1);
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        sim.queue(Command::SpawnGolem);
        sim.step();
        let stock = sim.stock;
        sim.queue(Command::RunProgram {
            golem: 1,
            source: format!(
                "fn main() {{\n    let tour = ({}, {});\n    aller_vers(qg());\n    let cristal = prendre().expect(\"rien au QG\");\n    aller_vers(tour);\n    match deposer(cristal) {{\n        Ok(()) => println!(\"fini {{}}\", munitions(tour)),\n        Err(_) => println!(\"raté\"),\n    }}\n}}",
                tile.x, tile.y
            ),
        });
        run(&mut sim, 200);
        let golem = sim.golem(1).unwrap();
        assert!(golem.rust);
        assert_eq!(golem.status, GolemStatus::Finished, "{:?}", golem.status);
        assert_eq!(golem.log, vec!["fini 6"]);
        assert_eq!(golem.carrying, None);
        assert_eq!(sim.stock, stock - 1);
    }

    #[test]
    fn rust_ownership_errors_stop_the_golem_before_it_runs() {
        let mut sim = Simulation::new(4, 96);
        sim.queue(Command::SpawnGolem);
        sim.queue(Command::RunProgram {
            golem: 1,
            source: "fn main() {\n    let o = prendre().unwrap();\n    let _ = deposer(o);\n    let _ = deposer(o);\n}".into(),
        });
        run(&mut sim, 3);
        let GolemStatus::Error(error) = &sim.golem(1).unwrap().status else {
            panic!("{:?}", sim.golem(1).unwrap().status);
        };
        assert_eq!(error.line, 4);
        assert!(error.message(gem_script::Lang::English).contains("E0382"));
    }

    #[test]
    fn golems_coordinate_with_reservations_and_messages() {
        let mut sim = Simulation::new(4, 96);
        sim.stock = 100;
        sim.queue(Command::SpawnGolem);
        sim.queue(Command::SpawnGolem);
        sim.step();
        sim.queue(Command::RunProgram {
            golem: 1,
            source: "dire(reserver(10, 10), mon_numero())\nenvoyer(7, 42)\nattendre(3)".into(),
        });
        sim.queue(Command::RunProgram {
            golem: 2,
            source: "fn main() {\n    attendre(1);\n    println!(\"{} {:?} {:?}\", reserver((10, 10)), recevoir(7), recevoir(7));\n}".into(),
        });
        run(&mut sim, 60);
        assert_eq!(sim.golem(1).unwrap().log, vec!["True 1"]);
        assert_eq!(sim.golem(2).unwrap().log, vec!["false Some(42) None"]);
        // Quand le programme du premier golem se termine, sa réservation est libérée.
        run(&mut sim, 120);
        assert!(sim.reservations.is_empty());
    }

    #[test]
    fn golem_errors_are_reported_with_their_line() {
        let mut sim = Simulation::new(4, 96);
        sim.queue(Command::SpawnGolem);
        sim.queue(Command::RunProgram {
            golem: 1,
            source: "x = 1\nmunitions(0, 0)".into(),
        });
        run(&mut sim, 3);
        let GolemStatus::Error(error) = &sim.golem(1).unwrap().status else {
            panic!("{:?}", sim.golem(1).unwrap().status);
        };
        assert_eq!(error.line, 2);
        assert!(
            error
                .message(gem_script::Lang::French)
                .contains("pas de tour")
        );
    }

    #[test]
    fn every_format_has_a_reachable_core_except_colony() {
        for seed in [1, 7, 2026] {
            for format in Format::ALL {
                let mut sim = Simulation::with_format(seed, format);
                sim.explored.fill(true);
                assert_eq!(sim.map.size, format.config().map_size);
                match format {
                    Format::Colony => assert!(sim.core.is_none()),
                    _ => {
                        let core = sim.core.unwrap_or_else(|| panic!("{format:?} {seed}"));
                        assert_eq!(
                            sim.check_build(BuildingKind::CoreDrill, core),
                            Err(BuildError::NotEnoughCrystals)
                        );
                        assert_eq!(
                            sim.check_build(BuildingKind::Tower, Tile::new(core.x + 1, core.y + 1)),
                            Err(BuildError::CoreSite)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn drilling_triggers_boss_waves_then_extraction_and_victory() {
        let mut sim = Simulation::with_format(5, Format::Scout);
        sim.explored.fill(true);
        let core = sim.core.unwrap();
        sim.stock = 1000;
        sim.queue(Command::Build {
            kind: BuildingKind::CoreDrill,
            tile: core,
            dir: Dir::South,
        });
        sim.step();
        let drill = sim.core_drill().unwrap().id;
        let refuel = |sim: &mut Simulation| {
            if let State::CoreDrill { fuel, .. } = &mut sim.buildings.get_mut(drill).unwrap().state
            {
                *fuel = 10;
            }
        };
        let per_stage = sim.config.crystals_per_stage;
        while sim.stage == 0 {
            refuel(&mut sim);
            sim.step();
        }
        assert_eq!(sim.drilled, per_stage);
        // Le premier palier déclenche une vague avec des boss.
        run(&mut sim, 30);
        assert!(sim.enemies.iter().any(|e| e.boss));
        while sim.extraction_until.is_none() {
            refuel(&mut sim);
            sim.enemies.clear();
            sim.step();
        }
        assert_eq!(sim.drilling_percent(), 100);
        while sim.outcome.is_none() {
            sim.enemies.clear();
            sim.step();
        }
        assert_eq!(sim.outcome, Some(Outcome::Victory));
    }

    #[test]
    fn colony_is_endless() {
        let mut sim = Simulation::with_format(3, Format::Colony);
        assert_eq!(
            sim.check_build(BuildingKind::CoreDrill, Tile::new(0, 0)),
            Err(BuildError::NoCore)
        );
        sim.buildings.get_mut(sim.hq().id).unwrap().hp = i32::MAX / 2;
        let day = sim.config.ticks_per_day as u64;
        run(&mut sim, day * 4);
        assert_eq!(sim.outcome, None);
        assert_eq!(sim.wave, 4);
    }

    #[test]
    fn research_bonuses_change_the_rules() {
        let config = ExpeditionConfig {
            bonuses: Bonuses {
                starting_crystals: 120,
                hq_hp: 300,
                golem_cost: 5,
                starting_gem: true,
                ..Bonuses::default()
            },
            ..Format::Scout.config()
        };
        let mut sim = Simulation::with_config(9, config);
        assert_eq!(sim.stock, 120);
        assert_eq!(sim.hq().max_hp, BuildingKind::Hq.max_hp() + 300);
        assert_eq!(sim.gems.len(), 1);
        assert_eq!(sim.gems[0].grade, 2);
        sim.queue(Command::SpawnGolem);
        sim.step();
        assert_eq!(sim.stock, 115);
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

    /// Première case libre (et découverte) autour de `near` pour ce bâtiment.
    fn free_tile(sim: &Simulation, kind: BuildingKind, near: Tile) -> Tile {
        for r in 2..14 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let tile = Tile::new(near.x + dx, near.y + dy);
                    if sim.check_build(kind, tile).is_ok() {
                        return tile;
                    }
                }
            }
        }
        panic!("pas de place pour {kind:?}");
    }

    fn build(sim: &mut Simulation, kind: BuildingKind) -> Tile {
        let tile = free_tile(sim, kind, sim.hq().tile);
        sim.queue(Command::Build {
            kind,
            tile,
            dir: Dir::East,
        });
        sim.step();
        assert!(sim.buildings.at(tile).is_some_and(|b| b.kind == kind));
        tile
    }

    #[test]
    fn power_shortage_slows_machines_until_a_plant_is_built() {
        let mut sim = Simulation::new(4, 96);
        sim.stock = 10_000;
        let mut shortages = 0;
        for _ in 0..4 {
            build(&mut sim, BuildingKind::Tower);
            shortages += sim
                .events
                .iter()
                .filter(|e| **e == SimEvent::PowerShortage)
                .count();
        }
        sim.step();
        assert_eq!(sim.power, (6, 8));
        assert_eq!(shortages, 1);
        let mut powered = 0;
        for _ in 0..40 {
            sim.step();
            powered += sim.buildings.powered as u32;
        }
        assert_eq!(powered, 30);
        build(&mut sim, BuildingKind::PowerPlant);
        sim.step();
        assert_eq!(sim.power, (6 + building::POWER_PLANT_OUTPUT, 8));
        for _ in 0..10 {
            sim.step();
            assert!(sim.buildings.powered);
        }
    }

    #[test]
    fn barracks_train_units_within_the_supply_limit() {
        let mut sim = Simulation::new(4, 96);
        sim.stock = 10_000;
        let hq = sim.hq().tile;
        assert_eq!(
            sim.check_build(
                BuildingKind::Barracks,
                free_tile(&sim, BuildingKind::Conveyor, hq)
            ),
            Err(BuildError::Requires(BuildingKind::PowerPlant))
        );
        build(&mut sim, BuildingKind::PowerPlant);
        let barracks = build(&mut sim, BuildingKind::Barracks);
        let before = sim.stock;
        for _ in 0..6 {
            sim.queue(Command::Train { tile: barracks });
        }
        sim.step();
        // Le QG loge 4 unités : la 5e et la 6e sont refusées.
        assert_eq!(sim.supply(), (4, building::HQ_SUPPLY));
        assert_eq!(before - sim.stock, 4 * UnitKind::Sentinel.stats().cost);
        let mut trained = 0;
        for _ in 0..4 * UnitKind::Sentinel.stats().train_ticks + 10 {
            sim.step();
            trained += sim
                .events
                .iter()
                .filter(|e| **e == SimEvent::UnitTrained)
                .count();
        }
        assert_eq!(trained, 4);
        let sentinels = sim
            .units
            .iter()
            .filter(|u| u.kind == UnitKind::Sentinel)
            .count();
        assert_eq!(sentinels, 4);
        // Un habitat en loge 4 de plus.
        build(&mut sim, BuildingKind::Habitat);
        assert_eq!(
            sim.supply(),
            (4, building::HQ_SUPPLY + building::HABITAT_SUPPLY)
        );
    }

    #[test]
    fn units_follow_orders_in_formation() {
        let mut sim = Simulation::new(4, 96);
        let hero = sim.hero().unwrap().id;
        let hq = sim.hq().tile;
        let target = Tile::new(hq.x + 8, hq.y + 8).center();
        sim.queue(Command::Order {
            units: vec![hero, hero, 12345],
            order: Order::Move(target),
        });
        run(&mut sim, 300);
        let hero = sim.hero().unwrap();
        assert_eq!(hero.order, Order::Idle);
        assert!(hero.pos.distance_sq(target) <= (3 * UNITS_PER_TILE as i64).pow(2));
    }

    #[test]
    fn an_army_destroys_nests_and_wins_the_colony() {
        let mut sim = Simulation::with_format(3, Format::Colony);
        let mut id = 100;
        for nest in sim.nests.clone() {
            for k in 0..12 {
                let mut unit = Unit::new(id, UnitKind::Tank, nest.center());
                unit.pos.x += (k % 4 - 2) * UNITS_PER_TILE / 2;
                unit.pos.y += (k / 4 - 1) * UNITS_PER_TILE / 2;
                sim.units.push(unit);
                id += 1;
            }
        }
        let stock = sim.stock;
        let mut destroyed = 0;
        for _ in 0..2000 {
            sim.step();
            destroyed += sim
                .events
                .iter()
                .filter(|e| matches!(e, SimEvent::NestDestroyed { .. }))
                .count();
            if sim.outcome.is_some() {
                break;
            }
        }
        assert_eq!(destroyed, sim.nests.len());
        assert!(sim.nest_hp.iter().all(|&hp| hp == 0));
        assert_eq!(sim.outcome, Some(Outcome::Victory));
        assert!(sim.stock >= stock + NEST_REWARD * sim.nests.len() as u32);
    }

    #[test]
    fn hero_abilities_have_levels_and_cooldowns() {
        let mut sim = Simulation::new(4, 96);
        let pos = sim.hero().unwrap().pos;
        let spec = wave_spec(1, 100);
        let mut enemy = Enemy::new(999, pos, &spec);
        enemy.hp = 10_000;
        sim.enemies.push(enemy);
        // La pluie de cristaux demande le niveau 3.
        sim.queue(Command::Ability(Ability::CrystalRain));
        sim.step();
        assert!(
            sim.events
                .iter()
                .all(|e| !matches!(e, SimEvent::AbilityUsed { .. }))
        );
        let hp = sim.enemies[0].hp;
        sim.queue(Command::Ability(Ability::FrostWave));
        sim.step();
        assert!(sim.events.iter().any(|e| matches!(
            e,
            SimEvent::AbilityUsed {
                ability: Ability::FrostWave,
                ..
            }
        )));
        assert!(hp - sim.enemies[0].hp >= Ability::FrostWave.damage(1));
        // Encore en recharge.
        sim.queue(Command::Ability(Ability::FrostWave));
        sim.step();
        assert!(
            sim.events
                .iter()
                .all(|e| !matches!(e, SimEvent::AbilityUsed { .. }))
        );
    }

    #[test]
    fn the_fallen_hero_comes_back_to_the_hq() {
        let mut sim = Simulation::new(4, 96);
        sim.units[0].hp = 0;
        sim.step();
        assert!(sim.hero().is_none());
        assert!(sim.events.contains(&SimEvent::UnitLost));
        run(&mut sim, unit::HERO_RESPAWN_TICKS);
        let hero = sim.hero().unwrap();
        assert_eq!(hero.hp, hero.max_hp);
    }

    #[test]
    fn fog_of_war_hides_far_tiles_until_explored() {
        let mut sim = Simulation::new(4, 96);
        let hq = sim.hq().tile;
        let far = Tile::new(hq.x + 30, hq.y);
        assert!(!sim.is_explored(far));
        assert_eq!(
            sim.check_build(BuildingKind::Conveyor, far),
            Err(BuildError::Unexplored)
        );
        let hero = sim.hero().unwrap().id;
        sim.queue(Command::Order {
            units: vec![hero],
            order: Order::Move(far.center()),
        });
        run(&mut sim, 600);
        assert!(sim.is_explored(far));
    }
}
