//! Bâtiments : foreuses, convoyeurs, tours et QG, et le transport du cristal entre eux.

use std::collections::BTreeMap;

use crate::{
    geom::{Dir, Pos, Tile, UNITS_PER_TILE},
    map::{Map, Terrain},
};

/// Une foreuse produit un cristal toutes les `DRILL_PERIOD` ticks (2 s).
pub const DRILL_PERIOD: u32 = 60;
const DRILL_BUFFER: u32 = 4;
/// Un objet traverse une case de convoyeur en `CONVEYOR_TICKS` ticks (2 cases/s).
pub const CONVEYOR_TICKS: u32 = 15;
/// Tirs fournis par un cristal chargé dans une tour.
const SHOTS_PER_CRYSTAL: u32 = 6;
const TOWER_MAX_CRYSTALS: u32 = 4;

/// Côté du carré occupé par le QG, en cases.
pub const HQ_SIZE: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Hq,
    Drill,
    Conveyor,
    Tower,
}

impl BuildingKind {
    /// Bâtiments que le joueur peut construire.
    pub const BUILDABLE: [BuildingKind; 3] = [
        BuildingKind::Drill,
        BuildingKind::Conveyor,
        BuildingKind::Tower,
    ];

    /// Coût en cristaux.
    pub const fn cost(self) -> u32 {
        match self {
            BuildingKind::Hq => 0,
            BuildingKind::Drill => 10,
            BuildingKind::Conveyor => 1,
            BuildingKind::Tower => 20,
        }
    }

    pub const fn max_hp(self) -> i32 {
        match self {
            BuildingKind::Hq => 600,
            BuildingKind::Drill => 120,
            BuildingKind::Conveyor => 40,
            BuildingKind::Tower => 200,
        }
    }
}

/// Caractéristiques d'une arme (tour ou défense du QG).
#[derive(Clone, Copy, Debug)]
pub struct Weapon {
    /// Portée, en unités fines.
    pub range: i32,
    pub damage: i32,
    /// Ticks entre deux tirs.
    pub cooldown: u32,
}

pub const TOWER_WEAPON: Weapon = Weapon {
    range: 7 * UNITS_PER_TILE,
    damage: 18,
    cooldown: 15,
};

/// Le QG se défend seul, faiblement, sans munitions.
pub const HQ_WEAPON: Weapon = Weapon {
    range: 6 * UNITS_PER_TILE,
    damage: 7,
    cooldown: 24,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildingState {
    Hq {
        cooldown: u32,
    },
    Drill {
        timer: u32,
        /// Cristaux produits en attente de sortie.
        buffer: u32,
    },
    Conveyor {
        /// Le convoyeur porte au plus un cristal, qui avance de 0 à `CONVEYOR_TICKS`.
        item: Option<u32>,
    },
    Tower {
        crystals: u32,
        /// Tirs restants avant de consommer un nouveau cristal.
        charge: u32,
        cooldown: u32,
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

    pub fn weapon(&self) -> Option<Weapon> {
        match self.kind {
            BuildingKind::Tower => Some(TOWER_WEAPON),
            BuildingKind::Hq => Some(HQ_WEAPON),
            _ => None,
        }
    }

    /// Peut-il recevoir un cristal venant de `from` ?
    fn accepts_from(&self, from: Tile) -> bool {
        match &self.state {
            BuildingState::Hq { .. } => true,
            // Un convoyeur n'accepte rien par sa sortie.
            BuildingState::Conveyor { item } => item.is_none() && self.output_tile() != from,
            BuildingState::Tower { crystals, .. } => *crystals < TOWER_MAX_CRYSTALS,
            BuildingState::Drill { .. } => false,
        }
    }
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

    pub fn at(&self, tile: Tile) -> Option<&Building> {
        let id = self.grid[self.index(tile)?];
        self.by_id.get(&id)
    }

    pub fn hq(&self) -> Option<&Building> {
        self.by_id.values().find(|b| b.kind == BuildingKind::Hq)
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
    pub fn place(&mut self, kind: BuildingKind, tile: Tile, dir: Dir) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let state = match kind {
            BuildingKind::Hq => BuildingState::Hq { cooldown: 0 },
            BuildingKind::Drill => BuildingState::Drill {
                timer: 0,
                buffer: 0,
            },
            BuildingKind::Conveyor => BuildingState::Conveyor { item: None },
            BuildingKind::Tower => BuildingState::Tower {
                crystals: 0,
                charge: 0,
                cooldown: 0,
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

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Building> {
        self.by_id.get_mut(&id)
    }

    pub fn ids(&self) -> Vec<u32> {
        self.by_id.keys().copied().collect()
    }

    /// Essaie de faire passer un cristal du bâtiment `from` vers la case `to`.
    /// Renvoie `true` si le cristal a été accepté. Les cristaux livrés au QG s'ajoutent à `stock`.
    fn hand_over(&mut self, from: Tile, to: Tile, stock: &mut u32) -> bool {
        let Some(index) = self.index(to) else {
            return false;
        };
        let Some(target) = self.by_id.get_mut(&self.grid[index]) else {
            return false;
        };
        if !target.accepts_from(from) {
            return false;
        }
        match &mut target.state {
            BuildingState::Hq { .. } => *stock += 1,
            BuildingState::Conveyor { item } => *item = Some(0),
            BuildingState::Tower { crystals, .. } => *crystals += 1,
            BuildingState::Drill { .. } => unreachable!(),
        }
        true
    }

    /// Production et transport du cristal, pour un tick.
    pub fn update_logistics(&mut self, stock: &mut u32) {
        for id in self.ids() {
            let building = &self.by_id[&id];
            let (from, to) = (building.tile, building.output_tile());
            match building.state.clone() {
                BuildingState::Drill {
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
                    if buffer > 0 && self.hand_over(from, to, stock) {
                        buffer -= 1;
                    }
                    self.by_id.get_mut(&id).unwrap().state = BuildingState::Drill { timer, buffer };
                }
                BuildingState::Conveyor {
                    item: Some(progress),
                } => {
                    let item = if progress < CONVEYOR_TICKS {
                        Some(progress + 1)
                    } else if self.hand_over(from, to, stock) {
                        None
                    } else {
                        Some(progress)
                    };
                    self.by_id.get_mut(&id).unwrap().state = BuildingState::Conveyor { item };
                }
                BuildingState::Tower {
                    crystals,
                    charge: 0,
                    cooldown,
                } if crystals > 0 => {
                    self.by_id.get_mut(&id).unwrap().state = BuildingState::Tower {
                        crystals: crystals - 1,
                        charge: SHOTS_PER_CRYSTAL,
                        cooldown,
                    };
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crystal_conveyor_line() -> (Map, Buildings, Tile) {
        let map = Map::generate(7, 256);
        let vein = map
            .tiles()
            .find(|&(x, y, t)| {
                t == Terrain::CrystalVein
                    && (1..4).all(|i| x + i < map.size && map.tile(x + i, y).is_buildable())
            })
            .map(|(x, y, _)| Tile::new(x as i32, y as i32))
            .expect("un filon avec de la place à l'est");
        (map, Buildings::new(256), vein)
    }

    #[test]
    fn drill_feeds_tower_through_conveyors() {
        let (map, mut buildings, vein) = crystal_conveyor_line();
        assert_eq!(
            buildings.check_site(&map, BuildingKind::Drill, vein),
            Ok(())
        );
        buildings.place(BuildingKind::Drill, vein, Dir::East);
        buildings.place(BuildingKind::Conveyor, vein.step(Dir::East), Dir::East);
        buildings.place(
            BuildingKind::Conveyor,
            vein.step(Dir::East).step(Dir::East),
            Dir::East,
        );
        let tower = buildings.place(
            BuildingKind::Tower,
            Tile::new(vein.x + 3, vein.y),
            Dir::East,
        );

        let mut stock = 0;
        for _ in 0..DRILL_PERIOD * 3 {
            buildings.update_logistics(&mut stock);
        }
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
    fn drills_only_on_crystal_veins() {
        let (map, buildings, vein) = crystal_conveyor_line();
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
        let (map, mut buildings, vein) = crystal_conveyor_line();
        let id = buildings.place(BuildingKind::Tower, vein, Dir::East);
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
