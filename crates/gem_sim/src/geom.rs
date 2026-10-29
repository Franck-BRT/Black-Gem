//! Positions et directions en entiers (virgule fixe), pour rester déterministe.

/// Unités de position par case : les positions sont en millièmes de case.
pub const UNITS_PER_TILE: i32 = 1000;

/// Coordonnées d'une case de la grille.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
}

impl Tile {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn step(self, dir: Dir) -> Tile {
        let (dx, dy) = dir.offset();
        Tile::new(self.x + dx, self.y + dy)
    }

    /// Centre de la case, en position fine.
    pub fn center(self) -> Pos {
        Pos::new(
            self.x * UNITS_PER_TILE + UNITS_PER_TILE / 2,
            self.y * UNITS_PER_TILE + UNITS_PER_TILE / 2,
        )
    }
}

/// Position fine sur la carte, en millièmes de case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn tile(self) -> Tile {
        Tile::new(
            self.x.div_euclid(UNITS_PER_TILE),
            self.y.div_euclid(UNITS_PER_TILE),
        )
    }

    pub fn distance_sq(self, other: Pos) -> i64 {
        let dx = (self.x - other.x) as i64;
        let dy = (self.y - other.y) as i64;
        dx * dx + dy * dy
    }

    /// Avance vers `target` d'au plus `step` unités. Renvoie `true` si la cible est atteinte.
    pub fn move_towards(&mut self, target: Pos, step: i32) -> bool {
        let distance = self.distance_sq(target).cast_unsigned().isqrt() as i64;
        if distance <= step as i64 {
            *self = target;
            return true;
        }
        self.x += ((target.x - self.x) as i64 * step as i64 / distance) as i32;
        self.y += ((target.y - self.y) as i64 * step as i64 / distance) as i32;
        false
    }
}

/// Orientation d'un bâtiment. Le nord est vers le haut de la carte (y décroissant).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    #[default]
    East,
    South,
    West,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

    pub const fn offset(self) -> (i32, i32) {
        match self {
            Dir::North => (0, -1),
            Dir::East => (1, 0),
            Dir::South => (0, 1),
            Dir::West => (-1, 0),
        }
    }

    /// Rotation d'un quart de tour dans le sens horaire.
    pub const fn clockwise(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
        }
    }

    pub const fn opposite(self) -> Dir {
        self.clockwise().clockwise()
    }

    /// Direction d'une case vers une case voisine.
    pub fn between(from: Tile, to: Tile) -> Option<Dir> {
        Dir::ALL.into_iter().find(|&d| from.step(d) == to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_and_positions() {
        let tile = Tile::new(3, -2);
        assert_eq!(tile.center().tile(), tile);
        assert_eq!(tile.step(Dir::North), Tile::new(3, -3));
        assert_eq!(Dir::between(tile, Tile::new(4, -2)), Some(Dir::East));
        assert_eq!(Dir::West.opposite(), Dir::East);
    }

    #[test]
    fn move_towards_reaches_target() {
        let mut p = Pos::new(0, 0);
        let target = Pos::new(3000, 4000);
        let mut steps = 0;
        while !p.move_towards(target, 500) {
            steps += 1;
        }
        assert_eq!(p, target);
        assert_eq!(steps, 9);
    }
}
