//! La Corruption : nids, vagues nocturnes et déplacement des ennemis vers le QG.

use std::collections::VecDeque;

use crate::{
    TICKS_PER_SECOND,
    gem::{POISON_TICKS, SLOW_TICKS},
    geom::{Dir, Pos, Tile, UNITS_PER_TILE},
    map::Map,
};

/// Distance (au QG) à partir de laquelle un ennemi attaque, en unités fines.
const ATTACK_RANGE: i32 = 2 * UNITS_PER_TILE + UNITS_PER_TILE / 2;
const ATTACK_DAMAGE: i32 = 10;
const ATTACK_COOLDOWN: u32 = 30;
/// Ticks entre deux apparitions au même nid.
pub const SPAWN_INTERVAL: u64 = 25;
pub const NEST_COUNT: usize = 3;

#[derive(Clone, Debug)]
pub struct Enemy {
    pub id: u32,
    pub pos: Pos,
    /// Position au tick précédent (pour que le rendu interpole le mouvement).
    pub prev_pos: Pos,
    pub hp: i32,
    pub max_hp: i32,
    /// Vitesse, en unités fines par tick.
    pub speed: i32,
    pub attack_cooldown: u32,
    /// Poison actif : points de vie perdus par seconde, et ticks restants.
    pub poison_per_second: i32,
    pub poison_ticks: u32,
    poison_carry: i32,
    /// Ralentissement actif, en pour cent, et ticks restants.
    pub slow_percent: i32,
    pub slow_ticks: u32,
}

/// Composition d'une vague.
pub struct WaveSpec {
    pub per_nest: u32,
    pub hp: i32,
    pub speed: i32,
}

/// Les vagues grossissent nuit après nuit.
pub fn wave_spec(wave: u32) -> WaveSpec {
    WaveSpec {
        per_nest: 2 + 2 * wave,
        hp: 30 + 30 * wave as i32 + 15 * (wave * wave) as i32,
        speed: 36 + 3 * wave as i32,
    }
}

/// Carte des distances au QG, case par case (parcours en largeur sur le terrain praticable).
/// Chaque ennemi descend simplement la pente des distances.
#[derive(Clone, Debug)]
pub struct FlowField {
    size: i32,
    distance: Vec<u32>,
}

impl FlowField {
    pub fn toward(map: &Map, goal: &[Tile]) -> Self {
        let size = map.size as i32;
        let mut distance = vec![u32::MAX; (size * size) as usize];
        let mut queue = VecDeque::new();
        for &tile in goal {
            distance[(tile.y * size + tile.x) as usize] = 0;
            queue.push_back(tile);
        }
        while let Some(tile) = queue.pop_front() {
            let d = distance[(tile.y * size + tile.x) as usize];
            for dir in Dir::ALL {
                let next = tile.step(dir);
                if !(0..size).contains(&next.x) || !(0..size).contains(&next.y) {
                    continue;
                }
                let index = (next.y * size + next.x) as usize;
                if distance[index] == u32::MAX
                    && map.tile(next.x as u32, next.y as u32).is_walkable()
                {
                    distance[index] = d + 1;
                    queue.push_back(next);
                }
            }
        }
        Self { size, distance }
    }

    pub fn distance(&self, tile: Tile) -> Option<u32> {
        if !(0..self.size).contains(&tile.x) || !(0..self.size).contains(&tile.y) {
            return None;
        }
        let d = self.distance[(tile.y * self.size + tile.x) as usize];
        (d != u32::MAX).then_some(d)
    }

    /// Case suivante pour se rapprocher du but (ordre des directions fixe : déterministe).
    pub fn next(&self, tile: Tile) -> Option<Tile> {
        let here = self.distance(tile)?;
        Dir::ALL
            .into_iter()
            .map(|dir| tile.step(dir))
            .filter_map(|t| Some((self.distance(t)?, t)))
            .filter(|&(d, _)| d < here)
            .min_by_key(|&(d, _)| d)
            .map(|(_, t)| t)
    }
}

/// Place les nids loin du QG et loin les uns des autres, sur des cases qui mènent au QG.
pub fn choose_nests(map: &Map, field: &FlowField) -> Vec<Tile> {
    let size = map.size as i32;
    let margin = 3;
    let candidates: Vec<Tile> = (0..size)
        .flat_map(|y| (0..size).map(move |x| Tile::new(x, y)))
        .filter(|t| {
            let near_edge =
                t.x < margin || t.y < margin || t.x >= size - margin || t.y >= size - margin;
            near_edge && field.distance(*t).is_some()
        })
        .collect();

    let mut nests: Vec<Tile> = Vec::new();
    while nests.len() < NEST_COUNT {
        // Le premier nid est le plus éloigné du QG ; les suivants, les plus éloignés des nids déjà choisis.
        let best = candidates.iter().copied().max_by_key(|&t| {
            let spread = nests
                .iter()
                .map(|n| ((t.x - n.x).pow(2) + (t.y - n.y).pow(2)) as u64)
                .min()
                .unwrap_or(u64::MAX / 2);
            (spread.min(u64::MAX / 2), field.distance(t).unwrap_or(0), t)
        });
        match best {
            Some(tile) => nests.push(tile),
            None => break,
        }
    }
    nests
}

impl Enemy {
    pub fn new(id: u32, pos: Pos, spec: &WaveSpec) -> Self {
        Self {
            id,
            pos,
            prev_pos: pos,
            hp: spec.hp,
            max_hp: spec.hp,
            speed: spec.speed,
            attack_cooldown: 0,
            poison_per_second: 0,
            poison_ticks: 0,
            poison_carry: 0,
            slow_percent: 0,
            slow_ticks: 0,
        }
    }

    /// Empoisonne (le poison le plus fort l'emporte, sa durée repart à zéro).
    pub fn poison(&mut self, per_second: i32) {
        self.poison_per_second = self.poison_per_second.max(per_second);
        self.poison_ticks = POISON_TICKS;
    }

    /// Ralentit (le ralentissement le plus fort l'emporte).
    pub fn slow(&mut self, percent: i32) {
        self.slow_percent = self.slow_percent.max(percent);
        self.slow_ticks = SLOW_TICKS;
    }

    /// Effets dans la durée : poison et fin du ralentissement.
    fn update_effects(&mut self) {
        if self.poison_ticks > 0 {
            self.poison_ticks -= 1;
            self.poison_carry += self.poison_per_second;
            self.hp -= self.poison_carry / TICKS_PER_SECOND as i32;
            self.poison_carry %= TICKS_PER_SECOND as i32;
        } else {
            self.poison_per_second = 0;
        }
        if self.slow_ticks > 0 {
            self.slow_ticks -= 1;
        } else {
            self.slow_percent = 0;
        }
    }

    /// Fait avancer l'ennemi. Renvoie les dégâts infligés au QG pendant ce tick.
    pub fn update(&mut self, field: &FlowField, hq_center: Pos) -> i32 {
        self.prev_pos = self.pos;
        self.update_effects();
        self.attack_cooldown = self.attack_cooldown.saturating_sub(1);
        if self.pos.distance_sq(hq_center) <= (ATTACK_RANGE as i64).pow(2) {
            if self.attack_cooldown == 0 {
                self.attack_cooldown = ATTACK_COOLDOWN;
                return ATTACK_DAMAGE;
            }
            return 0;
        }
        let target = match field.next(self.pos.tile()) {
            Some(tile) => tile.center(),
            // Sur une case du but (le QG) : on s'approche du centre.
            None => hq_center,
        };
        let speed = self.speed * (100 - self.slow_percent) / 100;
        self.pos.move_towards(target, speed);
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Terrain;

    #[test]
    fn flow_field_leads_to_goal() {
        let map = Map::generate(3, 96);
        let goal = Tile::new(48, 48);
        let field = FlowField::toward(&map, &[goal]);
        let start = (0..96)
            .map(|x| Tile::new(x, 2))
            .find(|t| field.distance(*t).is_some())
            .unwrap();
        let mut tile = start;
        let mut steps = 0;
        while let Some(next) = field.next(tile) {
            assert_ne!(map.tile(next.x as u32, next.y as u32), Terrain::Water);
            tile = next;
            steps += 1;
        }
        assert_eq!(tile, goal);
        assert_eq!(steps, field.distance(start).unwrap());
    }

    #[test]
    fn poison_hurts_over_time_and_slow_wears_off() {
        let spec = wave_spec(1);
        let mut enemy = Enemy::new(1, Pos::new(0, 0), &spec);
        enemy.poison(30);
        enemy.slow(50);
        for _ in 0..TICKS_PER_SECOND {
            enemy.update_effects();
        }
        assert_eq!(enemy.hp, spec.hp - 30);
        assert_eq!(enemy.slow_percent, 50);
        for _ in 0..POISON_TICKS {
            enemy.update_effects();
        }
        assert_eq!(
            enemy.hp,
            spec.hp - 30 * POISON_TICKS as i32 / TICKS_PER_SECOND as i32
        );
        assert_eq!((enemy.slow_percent, enemy.poison_per_second), (0, 0));
    }

    #[test]
    fn nests_are_spread_out_and_reachable() {
        let map = Map::generate(5, 128);
        let field = FlowField::toward(&map, &[Tile::new(64, 64)]);
        let nests = choose_nests(&map, &field);
        assert_eq!(nests.len(), NEST_COUNT);
        for (i, a) in nests.iter().enumerate() {
            assert!(field.distance(*a).unwrap() > 40);
            for b in &nests[i + 1..] {
                assert!((a.x - b.x).pow(2) + (a.y - b.y).pow(2) > 40 * 40);
            }
        }
    }
}
