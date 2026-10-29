//! Bâtiments et transport des objets entre eux : foreuses, convoyeurs, tailleries,
//! fours de fusion, tours et QG.

use std::collections::BTreeMap;

use crate::{
    gem::{Gem, GemColor, Item, MAX_GRADE, TowerProfile},
    geom::{Dir, Pos, Tile, UNITS_PER_TILE},
    map::{Map, Terrain},
};

/// Une foreuse produit un cristal toutes les `DRILL_PERIOD` ticks (2 s).
pub const DRILL_PERIOD: u32 = 60;
const DRILL_BUFFER: u32 = 4;
/// Un objet traverse une case de convoyeur en `CONVEYOR_TICKS` ticks (2 cases/s).
pub const CONVEYOR_TICKS: u32 = 15;
/// Tirs fournis par un cristal chargé dans une tour.
pub const SHOTS_PER_CRYSTAL: u32 = 6;
const TOWER_MAX_CRYSTALS: u32 = 4;
/// Cristaux de même couleur pour tailler une gemme de grade 1.
pub const CRYSTALS_PER_GEM: u8 = 3;
/// Durée de la taille d'une gemme (2 s).
pub const CUT_TICKS: u32 = 60;
/// Durée d'une fusion (4 s).
pub const FUSE_TICKS: u32 = 120;
const TOWER_COOLDOWN: u32 = 15;
const HQ_COOLDOWN: u32 = 24;

/// Côté du carré occupé par le QG, en cases.
pub const HQ_SIZE: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Hq,
    Drill,
    Conveyor,
    /// Taillerie : trois cristaux d'une couleur deviennent une gemme de grade 1.
    Cutter,
    /// Four de fusion : deux gemmes de même grade deviennent une gemme du grade supérieur.
    Furnace,
    Tower,
}

impl BuildingKind {
    /// Bâtiments que le joueur peut construire.
    pub const BUILDABLE: [BuildingKind; 5] = [
        BuildingKind::Drill,
        BuildingKind::Conveyor,
        BuildingKind::Cutter,
        BuildingKind::Furnace,
        BuildingKind::Tower,
    ];

    /// Coût en cristaux.
    pub const fn cost(self) -> u32 {
        match self {
            BuildingKind::Hq => 0,
            BuildingKind::Drill => 10,
            BuildingKind::Conveyor => 1,
            BuildingKind::Cutter => 15,
            BuildingKind::Furnace => 25,
            BuildingKind::Tower => 20,
        }
    }

    pub const fn max_hp(self) -> i32 {
        match self {
            BuildingKind::Hq => 600,
            BuildingKind::Drill => 120,
            BuildingKind::Conveyor => 40,
            BuildingKind::Cutter | BuildingKind::Furnace => 150,
            BuildingKind::Tower => 200,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildingState {
    Hq {
        cooldown: u32,
    },
    Drill {
        /// Couleur du filon sous la foreuse.
        color: GemColor,
        timer: u32,
        /// Cristaux produits en attente de sortie.
        buffer: u32,
    },
    Conveyor {
        /// Le convoyeur porte au plus un objet, qui avance de 0 à `CONVEYOR_TICKS`.
        item: Option<(Item, u32)>,
    },
    Cutter {
        /// Cristaux en attente, par couleur.
        counts: [u8; 5],
        /// Couleur en cours de taille et avancement.
        cutting: Option<(GemColor, u32)>,
        output: Option<Gem>,
    },
    Furnace {
        slots: [Option<Gem>; 2],
        progress: u32,
        output: Option<Gem>,
    },
    Tower {
        crystals: u32,
        /// Tirs restants avant de consommer un nouveau cristal.
        charge: u32,
        cooldown: u32,
        /// Gemme sertie : elle donne à la tour sa couleur, ses effets et sa puissance.
        gem: Option<Gem>,
    },
}

#[derive(Clone, Debug)]
pub struct Building {
    pub id: u32,
    pub kind: BuildingKind,
    /// Case d'origine (coin nord-ouest pour le QG).
    pub tile: Tile,
    pub dir: Dir,
    pub hp: i32,
    pub state: BuildingState,
}

impl Building {
    pub fn size(&self) -> i32 {
        if self.kind == BuildingKind::Hq {
            HQ_SIZE
        } else {
            1
        }
    }

    /// Centre du bâtiment, en position fine.
    pub fn center(&self) -> Pos {
        let half = self.size() * UNITS_PER_TILE / 2;
        Pos::new(
            self.tile.x * UNITS_PER_TILE + half,
            self.tile.y * UNITS_PER_TILE + half,
        )
    }

    /// Case vers laquelle le bâtiment envoie ses objets.
    pub fn output_tile(&self) -> Tile {
        self.tile.step(self.dir)
    }

    /// Arme du bâtiment et délai entre deux tirs.
    pub fn weapon(&self) -> Option<(TowerProfile, u32)> {
        match self.state {
            BuildingState::Tower { gem, .. } => {
                Some((gem.map(Gem::profile).unwrap_or_default(), TOWER_COOLDOWN))
            }
            // Le QG se défend seul, faiblement, sans munitions.
            BuildingState::Hq { .. } => Some((
                TowerProfile {
                    damage: 7,
                    range: 6 * UNITS_PER_TILE,
                    ..TowerProfile::default()
                },
                HQ_COOLDOWN,
            )),
            _ => None,
        }
    }

    /// Peut-il recevoir cet objet venant de `from` ?
    fn accepts(&self, from: Tile, item: Item) -> bool {
        match (&self.state, item) {
            (BuildingState::Hq { .. }, _) => true,
            // Un convoyeur n'accepte rien par sa sortie.
            (BuildingState::Conveyor { item }, _) => item.is_none() && self.output_tile() != from,
            (BuildingState::Cutter { counts, .. }, Item::Crystal(color)) => {
                counts[color.index()] < CRYSTALS_PER_GEM
            }
            (BuildingState::Furnace { slots, .. }, Item::Gem(gem)) => {
                gem.grade < MAX_GRADE
                    && match slots {
                        [None, None] => true,
                        [Some(held), None] | [None, Some(held)] => held.grade == gem.grade,
                        [Some(_), Some(_)] => false,
                    }
            }
            (BuildingState::Tower { crystals, .. }, Item::Crystal(_) | Item::Shard) => {
                *crystals < TOWER_MAX_CRYSTALS
            }
            (BuildingState::Tower { gem, .. }, Item::Gem(_)) => gem.is_none(),
            _ => false,
        }
    }

    fn receive(&mut self, item: Item, storage: &mut Storage) {
        match (&mut self.state, item) {
            (BuildingState::Hq { .. }, Item::Crystal(_) | Item::Shard) => *storage.crystals += 1,
            (BuildingState::Hq { .. }, Item::Gem(gem)) => storage.gems.push(gem),
            (BuildingState::Conveyor { item: slot }, item) => *slot = Some((item, 0)),
            (BuildingState::Cutter { counts, .. }, Item::Crystal(color)) => {
                counts[color.index()] += 1
            }
            (BuildingState::Furnace { slots, .. }, Item::Gem(gem)) => {
                let free = slots.iter_mut().find(|s| s.is_none()).unwrap();
                *free = Some(gem);
            }
            (BuildingState::Tower { crystals, .. }, Item::Crystal(_) | Item::Shard) => {
                *crystals += 1
            }
            (BuildingState::Tower { gem: slot, .. }, Item::Gem(gem)) => *slot = Some(gem),
            _ => unreachable!("objet refusé par `accepts`"),
        }
    }
}

/// Réserves du QG : cristaux (monnaie de construction) et gemmes (inventaire).
pub struct Storage<'a> {
    pub crystals: &'a mut u32,
    pub gems: &'a mut Vec<Gem>,
}

/// Pourquoi une construction est refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    OutOfBounds,
    Occupied,
    Water,
    NeedsCrystalVein,
    NotEnoughCrystals,
}

/// Tous les bâtiments, dans un ordre déterministe (par identifiant).
#[derive(Clone, Debug)]
pub struct Buildings {
    by_id: BTreeMap<u32, Building>,
    /// Identifiant du bâtiment sur chaque case (0 = libre).
    grid: Vec<u32>,
    size: i32,
    next_id: u32,
    /// Incrémenté à chaque construction ou démolition (pour le rendu).
    pub version: u64,
}

impl Buildings {
    pub fn new(map_size: u32) -> Self {
        Self {
            by_id: BTreeMap::new(),
            grid: vec![0; (map_size * map_size) as usize],
            size: map_size as i32,
            next_id: 1,
            version: 0,
        }
    }

    fn index(&self, tile: Tile) -> Option<usize> {
        let inside = (0..self.size).contains(&tile.x) && (0..self.size).contains(&tile.y);
        inside.then(|| (tile.y * self.size + tile.x) as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Building> {
        self.by_id.values()
    }

    pub fn get(&self, id: u32) -> Option<&Building> {
        self.by_id.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Building> {
        self.by_id.get_mut(&id)
    }

    pub fn at(&self, tile: Tile) -> Option<&Building> {
        let id = self.grid[self.index(tile)?];
        self.by_id.get(&id)
    }

    pub fn hq(&self) -> Option<&Building> {
        self.by_id.values().find(|b| b.kind == BuildingKind::Hq)
    }

    pub fn ids(&self) -> Vec<u32> {
        self.by_id.keys().copied().collect()
    }

    /// Vérifie qu'un bâtiment peut être posé ici (sans tenir compte du coût).
    pub fn check_site(&self, map: &Map, kind: BuildingKind, tile: Tile) -> Result<(), BuildError> {
        let size = if kind == BuildingKind::Hq { HQ_SIZE } else { 1 };
        for dy in 0..size {
            for dx in 0..size {
                let t = Tile::new(tile.x + dx, tile.y + dy);
                let index = self.index(t).ok_or(BuildError::OutOfBounds)?;
                if self.grid[index] != 0 {
                    return Err(BuildError::Occupied);
                }
                let terrain = map.tile(t.x as u32, t.y as u32);
                if !terrain.is_buildable() {
                    return Err(BuildError::Water);
                }
                if kind == BuildingKind::Drill && terrain != Terrain::CrystalVein {
                    return Err(BuildError::NeedsCrystalVein);
                }
            }
        }
        Ok(())
    }

    /// Pose un bâtiment (le site doit avoir été vérifié).
    pub fn place(&mut self, map: &Map, kind: BuildingKind, tile: Tile, dir: Dir) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let state = match kind {
            BuildingKind::Hq => BuildingState::Hq { cooldown: 0 },
            BuildingKind::Drill => BuildingState::Drill {
                color: map.vein_color(tile.x as u32, tile.y as u32),
                timer: 0,
                buffer: 0,
            },
            BuildingKind::Conveyor => BuildingState::Conveyor { item: None },
            BuildingKind::Cutter => BuildingState::Cutter {
                counts: [0; 5],
                cutting: None,
                output: None,
            },
            BuildingKind::Furnace => BuildingState::Furnace {
                slots: [None, None],
                progress: 0,
                output: None,
            },
            BuildingKind::Tower => BuildingState::Tower {
                crystals: 0,
                charge: 0,
                cooldown: 0,
                gem: None,
            },
        };
        let building = Building {
            id,
            kind,
            tile,
            dir,
            hp: kind.max_hp(),
            state,
        };
        let size = building.size();
        for dy in 0..size {
            for dx in 0..size {
                let index = self.index(Tile::new(tile.x + dx, tile.y + dy)).unwrap();
                self.grid[index] = id;
            }
        }
        self.by_id.insert(id, building);
        self.version += 1;
        id
    }

    pub fn remove(&mut self, id: u32) -> Option<Building> {
        let building = self.by_id.remove(&id)?;
        for cell in self.grid.iter_mut().filter(|cell| **cell == id) {
            *cell = 0;
        }
        self.version += 1;
        Some(building)
    }

    /// Un golem prend l'objet disponible dans le bâtiment de cette case (sortie d'une
    /// foreuse, d'une taillerie ou d'un four, objet sur un convoyeur).
    pub fn take_at(&mut self, tile: Tile) -> Option<Item> {
        let index = self.index(tile)?;
        let building = self.by_id.get_mut(&self.grid[index])?;
        match &mut building.state {
            BuildingState::Drill { color, buffer, .. } if *buffer > 0 => {
                *buffer -= 1;
                Some(Item::Crystal(*color))
            }
            BuildingState::Conveyor { item } => item.take().map(|(item, _)| item),
            BuildingState::Cutter { output, .. } | BuildingState::Furnace { output, .. } => {
                output.take().map(Item::Gem)
            }
            _ => None,
        }
    }

    /// Un golem dépose un objet dans le bâtiment de cette case. Renvoie `false` si refusé.
    pub fn give_at(&mut self, tile: Tile, item: Item, storage: &mut Storage) -> bool {
        let Some(index) = self.index(tile) else {
            return false;
        };
        let Some(building) = self.by_id.get_mut(&self.grid[index]) else {
            return false;
        };
        // Déposé « d'en haut » : on fait comme s'il venait de la case elle-même.
        if !building.accepts(tile.step(building.dir.opposite()), item) {
            return false;
        }
        building.receive(item, storage);
        true
    }

    /// Essaie de faire passer un objet du bâtiment en `from` vers la case `to`.
    /// Renvoie `true` si l'objet a été accepté.
    fn hand_over(&mut self, from: Tile, to: Tile, item: Item, storage: &mut Storage) -> bool {
        let Some(index) = self.index(to) else {
            return false;
        };
        let Some(target) = self.by_id.get_mut(&self.grid[index]) else {
            return false;
        };
        if !target.accepts(from, item) {
            return false;
        }
        target.receive(item, storage);
        true
    }

    /// Essaie de sortir l'objet en attente d'un bâtiment ; le vide s'il est accepté.
    fn push_out(&mut self, from: Tile, to: Tile, slot: &mut Option<Gem>, storage: &mut Storage) {
        if let Some(gem) = *slot
            && self.hand_over(from, to, Item::Gem(gem), storage)
        {
            *slot = None;
        }
    }

    /// Production, transformation et transport des objets, pour un tick.
    pub fn update_logistics(&mut self, storage: &mut Storage) {
        for id in self.ids() {
            let building = &self.by_id[&id];
            let (from, to) = (building.tile, building.output_tile());
            let state = match building.state.clone() {
                BuildingState::Drill {
                    color,
                    mut timer,
                    mut buffer,
                } => {
                    if buffer < DRILL_BUFFER {
                        timer += 1;
                        if timer >= DRILL_PERIOD {
                            timer = 0;
                            buffer += 1;
                        }
                    }
                    if buffer > 0 && self.hand_over(from, to, Item::Crystal(color), storage) {
                        buffer -= 1;
                    }
                    BuildingState::Drill {
                        color,
                        timer,
                        buffer,
                    }
                }
                BuildingState::Conveyor {
                    item: Some((item, progress)),
                } => {
                    let item = if progress < CONVEYOR_TICKS {
                        Some((item, progress + 1))
                    } else if self.hand_over(from, to, item, storage) {
                        None
                    } else {
                        Some((item, progress))
                    };
                    BuildingState::Conveyor { item }
                }
                BuildingState::Cutter {
                    mut counts,
                    mut cutting,
                    mut output,
                } => {
                    self.push_out(from, to, &mut output, storage);
                    match cutting {
                        Some((color, t)) if t + 1 >= CUT_TICKS => {
                            if output.is_none() {
                                output = Some(Gem::new(color, 1));
                                cutting = None;
                            }
                        }
                        Some((color, t)) => cutting = Some((color, t + 1)),
                        None => {
                            if let Some(color) = GemColor::ALL
                                .into_iter()
                                .find(|c| counts[c.index()] >= CRYSTALS_PER_GEM)
                            {
                                counts[color.index()] -= CRYSTALS_PER_GEM;
                                cutting = Some((color, 0));
                            }
                        }
                    }
                    BuildingState::Cutter {
                        counts,
                        cutting,
                        output,
                    }
                }
                BuildingState::Furnace {
                    mut slots,
                    mut progress,
                    mut output,
                } => {
                    self.push_out(from, to, &mut output, storage);
                    if let [Some(a), Some(b)] = slots {
                        progress += 1;
                        if progress >= FUSE_TICKS && output.is_none() {
                            output = a.fuse(b);
                            slots = [None, None];
                            progress = 0;
                        }
                    }
                    BuildingState::Furnace {
                        slots,
                        progress,
                        output,
                    }
                }
                BuildingState::Tower {
                    crystals,
                    charge: 0,
                    cooldown,
                    gem,
                } if crystals > 0 => BuildingState::Tower {
                    crystals: crystals - 1,
                    charge: SHOTS_PER_CRYSTAL,
                    cooldown,
                    gem,
                },
                other => other,
            };
            self.by_id.get_mut(&id).unwrap().state = state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un filon avec trois cases constructibles à l'est.
    fn vein_with_room(map: &Map, room: u32) -> Tile {
        map.tiles()
            .find(|&(x, y, t)| {
                t == Terrain::CrystalVein
                    && (1..=room).all(|i| x + i < map.size && map.tile(x + i, y).is_buildable())
            })
            .map(|(x, y, _)| Tile::new(x as i32, y as i32))
            .expect("un filon avec de la place à l'est")
    }

    fn east(tile: Tile, n: i32) -> Tile {
        Tile::new(tile.x + n, tile.y)
    }

    fn run(buildings: &mut Buildings, ticks: u32) -> (u32, Vec<Gem>) {
        let (mut crystals, mut gems) = (0, Vec::new());
        for _ in 0..ticks {
            buildings.update_logistics(&mut Storage {
                crystals: &mut crystals,
                gems: &mut gems,
            });
        }
        (crystals, gems)
    }

    #[test]
    fn drill_feeds_tower_through_conveyors() {
        let map = Map::generate(7, 256);
        let vein = vein_with_room(&map, 3);
        let mut buildings = Buildings::new(256);
        buildings.place(&map, BuildingKind::Drill, vein, Dir::East);
        buildings.place(&map, BuildingKind::Conveyor, east(vein, 1), Dir::East);
        buildings.place(&map, BuildingKind::Conveyor, east(vein, 2), Dir::East);
        let tower = buildings.place(&map, BuildingKind::Tower, east(vein, 3), Dir::East);

        run(&mut buildings, DRILL_PERIOD * 3);
        let BuildingState::Tower {
            crystals, charge, ..
        } = buildings.get(tower).unwrap().state
        else {
            panic!()
        };
        // Le premier cristal est chargé en tirs, les suivants attendent dans la tour.
        assert_eq!(charge, SHOTS_PER_CRYSTAL);
        assert!(crystals >= 1, "{crystals}");
    }

    #[test]
    fn cutter_turns_crystals_into_a_gem_of_the_vein_color() {
        let map = Map::generate(7, 256);
        let vein = vein_with_room(&map, 2);
        let color = map.vein_color(vein.x as u32, vein.y as u32);
        let mut buildings = Buildings::new(256);
        buildings.place(&map, BuildingKind::Drill, vein, Dir::East);
        buildings.place(&map, BuildingKind::Cutter, east(vein, 1), Dir::East);
        let tower = buildings.place(&map, BuildingKind::Tower, east(vein, 2), Dir::East);

        run(&mut buildings, DRILL_PERIOD * 3 + CUT_TICKS + 5);
        let BuildingState::Tower { gem, .. } = buildings.get(tower).unwrap().state else {
            panic!()
        };
        assert_eq!(gem, Some(Gem::new(color, 1)));
    }

    #[test]
    fn furnace_fuses_two_gems_of_the_same_grade() {
        let map = Map::generate(7, 256);
        let site = vein_with_room(&map, 1);
        let mut buildings = Buildings::new(256);
        let furnace = buildings.place(&map, BuildingKind::Furnace, site, Dir::East);
        let red = Gem::new(GemColor::Red, 2);
        let cyan = Gem::new(GemColor::Cyan, 2);
        let state = &mut buildings.get_mut(furnace).unwrap();
        assert!(state.accepts(site, Item::Gem(red)));
        state.receive(
            Item::Gem(red),
            &mut Storage {
                crystals: &mut 0,
                gems: &mut Vec::new(),
            },
        );
        // Un grade différent est refusé, le même grade est accepté.
        assert!(!state.accepts(site, Item::Gem(Gem::new(GemColor::Cyan, 1))));
        assert!(state.accepts(site, Item::Gem(cyan)));
        state.receive(
            Item::Gem(cyan),
            &mut Storage {
                crystals: &mut 0,
                gems: &mut Vec::new(),
            },
        );

        // Le résultat sort vers une tour voisine.
        let tower = buildings.place(&map, BuildingKind::Tower, east(site, 1), Dir::East);
        run(&mut buildings, FUSE_TICKS + 2);
        let BuildingState::Tower { gem, .. } = buildings.get(tower).unwrap().state else {
            panic!()
        };
        assert_eq!(gem, red.fuse(cyan));
    }

    #[test]
    fn drills_only_on_crystal_veins() {
        let map = Map::generate(7, 256);
        let vein = vein_with_room(&map, 1);
        let buildings = Buildings::new(256);
        let plain = map
            .tiles()
            .find(|&(_, _, t)| t == Terrain::Plain)
            .map(|(x, y, _)| Tile::new(x as i32, y as i32))
            .unwrap();
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Drill, plain),
            Err(BuildError::NeedsCrystalVein)
        );
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Tower, vein),
            Ok(())
        );
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Tower, Tile::new(-1, 0)),
            Err(BuildError::OutOfBounds)
        );
    }

    #[test]
    fn removing_frees_the_tiles() {
        let map = Map::generate(7, 256);
        let vein = vein_with_room(&map, 1);
        let mut buildings = Buildings::new(256);
        let id = buildings.place(&map, BuildingKind::Tower, vein, Dir::East);
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Tower, vein),
            Err(BuildError::Occupied)
        );
        buildings.remove(id);
        assert!(buildings.at(vein).is_none());
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Tower, vein),
            Ok(())
        );
    }
}
