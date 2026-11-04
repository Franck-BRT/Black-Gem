//! Unités de combat (façon RTS classique) : soldats formés à la caserne, et le héros.
//!
//! Une unité obéit à un ordre (aller, attaquer en avançant, tenir la position) et attaque
//! d'elle-même les ennemis qui passent à portée. Le héros, le Forgeron-Ingénieur, gagne de
//! l'expérience en combattant et lance deux pouvoirs liés aux gemmes.

use crate::{
    TICKS_PER_SECOND,
    geom::{Pos, UNITS_PER_TILE},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    /// Sentinelle des Forgerons : tir à distance, rapide.
    Sentinel,
    /// Blindé des Colons : lent, très résistant, frappe fort.
    Tank,
    /// Le Forgeron-Ingénieur.
    Hero,
}

/// Caractéristiques d'une unité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitStats {
    pub hp: i32,
    pub damage: i32,
    /// Portée d'attaque, en unités fines.
    pub range: i32,
    /// Ticks entre deux attaques.
    pub reload: u32,
    /// Vitesse, en unités fines par tick.
    pub speed: i32,
    pub cost: u32,
    /// Place prise dans le ravitaillement (habitats).
    pub supply: u32,
    /// Ticks de formation à la caserne.
    pub train_ticks: u32,
    /// Rayon de vision, en cases (brouillard de guerre).
    pub sight: i32,
}

impl UnitKind {
    pub const fn stats(self) -> UnitStats {
        match self {
            UnitKind::Sentinel => UnitStats {
                hp: 140,
                damage: 14,
                range: 4 * UNITS_PER_TILE,
                reload: 20,
                speed: 75,
                cost: 25,
                supply: 1,
                train_ticks: 8 * TICKS_PER_SECOND,
                sight: 7,
            },
            UnitKind::Tank => UnitStats {
                hp: 420,
                damage: 34,
                range: 3 * UNITS_PER_TILE,
                reload: 40,
                speed: 48,
                cost: 45,
                supply: 2,
                train_ticks: 14 * TICKS_PER_SECOND,
                sight: 6,
            },
            UnitKind::Hero => UnitStats {
                hp: 500,
                damage: 22,
                range: 5 * UNITS_PER_TILE,
                reload: 15,
                speed: 85,
                cost: 0,
                supply: 0,
                train_ticks: 0,
                sight: 9,
            },
        }
    }
}

/// Ordre donné à une unité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Attend, et riposte aux ennemis proches.
    Idle,
    /// Va à un point sans s'arrêter pour combattre.
    Move(Pos),
    /// Va à un point en attaquant tout ce qu'elle croise (et les nids).
    AttackMove(Pos),
}

/// Distance à laquelle une unité au repos repère un ennemi et va l'attaquer.
const AGGRO_RANGE: i32 = 7 * UNITS_PER_TILE;
/// Le héros ressuscite au QG après ce délai.
pub const HERO_RESPAWN_TICKS: u64 = 30 * TICKS_PER_SECOND as u64;
/// Expérience par niveau du héros.
pub const XP_PER_LEVEL: u32 = 100;
pub const MAX_HERO_LEVEL: u32 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: u32,
    pub kind: UnitKind,
    pub pos: Pos,
    pub prev_pos: Pos,
    pub hp: i32,
    pub max_hp: i32,
    pub order: Order,
    pub cooldown: u32,
    /// Héros : niveau et expérience.
    pub level: u32,
    pub xp: u32,
}

/// Ce qu'une unité voit autour d'elle pendant son tick.
pub struct Surroundings<'a> {
    /// Ennemis : (identifiant, position).
    pub enemies: &'a [(u32, Pos)],
    /// Nids encore debout : (indice, position).
    pub nests: &'a [(usize, Pos)],
}

/// Ce qu'une unité frappe pendant un tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strike {
    Nothing,
    Enemy(u32, i32),
    Nest(usize, i32),
}

impl Unit {
    pub fn new(id: u32, kind: UnitKind, pos: Pos) -> Self {
        let stats = kind.stats();
        Self {
            id,
            kind,
            pos,
            prev_pos: pos,
            hp: stats.hp,
            max_hp: stats.hp,
            order: Order::Idle,
            cooldown: 0,
            level: 1,
            xp: 0,
        }
    }

    /// Dégâts, en tenant compte du niveau du héros.
    pub fn damage(&self) -> i32 {
        let base = self.kind.stats().damage;
        if self.kind == UnitKind::Hero {
            base + 5 * (self.level as i32 - 1)
        } else {
            base
        }
    }

    /// Le héros gagne de l'expérience ; renvoie vrai s'il monte de niveau.
    pub fn gain_xp(&mut self, xp: u32) -> bool {
        if self.kind != UnitKind::Hero || self.level >= MAX_HERO_LEVEL {
            return false;
        }
        self.xp += xp;
        let level = (1 + self.xp / XP_PER_LEVEL).min(MAX_HERO_LEVEL);
        if level > self.level {
            let gained = (level - self.level) as i32;
            self.level = level;
            self.max_hp += 60 * gained;
            self.hp = self.max_hp;
            return true;
        }
        false
    }

    /// Un tick : se déplacer selon l'ordre, et attaquer ce qui est à portée.
    pub fn update(&mut self, around: &Surroundings) -> Strike {
        self.prev_pos = self.pos;
        self.cooldown = self.cooldown.saturating_sub(1);
        let stats = self.kind.stats();
        let range_sq = (stats.range as i64).pow(2);
        let nearest = around
            .enemies
            .iter()
            .map(|&(id, p)| (p.distance_sq(self.pos), id, p))
            .min_by_key(|&(d, id, _)| (d, id));
        let fighting = !matches!(self.order, Order::Move(_));
        if fighting && let Some((d, id, p)) = nearest {
            if d <= range_sq {
                return self.attack(Strike::Enemy(id, self.damage()));
            }
            // Au repos ou en marche d'attaque : on va chercher l'ennemi repéré.
            if d <= (AGGRO_RANGE as i64).pow(2) {
                self.pos.move_towards(p, stats.speed);
                return Strike::Nothing;
            }
        }
        if let Order::AttackMove(_) | Order::Idle = self.order
            && let Some(&(nest, p)) = around
                .nests
                .iter()
                .min_by_key(|(i, p)| (p.distance_sq(self.pos), *i))
            && p.distance_sq(self.pos) <= range_sq + (UNITS_PER_TILE as i64).pow(2)
        {
            return self.attack(Strike::Nest(nest, self.damage()));
        }
        match self.order {
            Order::Move(target) | Order::AttackMove(target) => {
                if self.pos.move_towards(target, stats.speed) {
                    self.order = Order::Idle;
                }
            }
            Order::Idle => {}
        }
        Strike::Nothing
    }

    fn attack(&mut self, strike: Strike) -> Strike {
        if self.cooldown > 0 {
            return Strike::Nothing;
        }
        self.cooldown = self.kind.stats().reload;
        strike
    }
}

/// Pouvoirs du héros.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ability {
    /// Onde de gel : ralentit et blesse les ennemis autour du héros.
    FrostWave,
    /// Pluie de cristaux : gros dégâts autour du héros (à partir du niveau 3).
    CrystalRain,
}

impl Ability {
    pub const ALL: [Ability; 2] = [Ability::FrostWave, Ability::CrystalRain];

    pub const fn cooldown(self) -> u64 {
        match self {
            Ability::FrostWave => 20 * TICKS_PER_SECOND as u64,
            Ability::CrystalRain => 35 * TICKS_PER_SECOND as u64,
        }
    }

    pub const fn radius(self) -> i32 {
        match self {
            Ability::FrostWave => 6 * UNITS_PER_TILE,
            Ability::CrystalRain => 4 * UNITS_PER_TILE,
        }
    }

    pub const fn min_level(self) -> u32 {
        match self {
            Ability::FrostWave => 1,
            Ability::CrystalRain => 3,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Dégâts selon le niveau du héros.
    pub fn damage(self, level: u32) -> i32 {
        match self {
            Ability::FrostWave => 20 + 10 * level as i32,
            Ability::CrystalRain => 80 + 40 * level as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Tile;

    #[test]
    fn units_walk_then_fight() {
        let mut unit = Unit::new(1, UnitKind::Sentinel, Tile::new(0, 0).center());
        unit.order = Order::AttackMove(Tile::new(20, 0).center());
        let enemy_at = Tile::new(25, 0).center();
        let around = Surroundings {
            enemies: &[(7, enemy_at)],
            nests: &[],
        };
        let mut strikes = 0;
        for _ in 0..400 {
            if let Strike::Enemy(7, damage) = unit.update(&around) {
                assert_eq!(damage, 14);
                strikes += 1;
            }
        }
        assert!(strikes > 3, "{strikes}");
        // Elle s'est approchée à portée de l'ennemi.
        assert!(unit.pos.distance_sq(enemy_at) <= (unit.kind.stats().range as i64).pow(2));
    }

    #[test]
    fn a_move_order_ignores_enemies() {
        let mut unit = Unit::new(1, UnitKind::Tank, Tile::new(0, 0).center());
        let target = Tile::new(5, 0).center();
        unit.order = Order::Move(target);
        let around = Surroundings {
            enemies: &[(1, Tile::new(1, 0).center())],
            nests: &[],
        };
        for _ in 0..200 {
            if unit.order == Order::Idle {
                break;
            }
            assert_eq!(unit.update(&around), Strike::Nothing);
        }
        assert_eq!(unit.pos, target);
        assert_eq!(unit.order, Order::Idle);
    }

    #[test]
    fn the_hero_levels_up() {
        let mut hero = Unit::new(1, UnitKind::Hero, Tile::new(0, 0).center());
        assert!(!hero.gain_xp(50));
        assert!(hero.gain_xp(60));
        assert_eq!(hero.level, 2);
        assert_eq!(hero.damage(), 27);
        assert_eq!(hero.max_hp, 560);
    }
}
