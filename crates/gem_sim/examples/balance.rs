//! Équilibrage : joue automatiquement quelques stratégies et affiche le résultat.
//! `cargo run -p gem_sim --example balance`

use gem_sim::{BuildingKind, BuildingState, Command, Dir, Simulation, TICKS_PER_DAY, Tile};

fn play(seed: u64, towers: i32) -> String {
    let mut sim = Simulation::new(seed, 192);
    let hq = sim.hq().tile;
    // Des tours autour du QG, approvisionnées « par magie » (on teste la puissance de feu, pas la logistique).
    let spots = [
        (-2, -2),
        (4, -2),
        (-2, 4),
        (4, 4),
        (1, -3),
        (1, 5),
        (-3, 1),
        (5, 1),
    ];
    for &(dx, dy) in spots.iter().take(towers as usize) {
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile: Tile::new(hq.x + dx, hq.y + dy),
            dir: Dir::East,
        });
    }
    sim.stock = 1000;
    let mut night_hp = Vec::new();
    while sim.outcome.is_none() && sim.tick < TICKS_PER_DAY as u64 * 5 {
        for id in sim.buildings.ids() {
            if let Some(b) = sim.buildings.get_mut(id)
                && let BuildingState::Tower { crystals, .. } = &mut b.state
            {
                *crystals = 4;
            }
        }
        sim.step();
        if sim.tick % TICKS_PER_DAY as u64 == TICKS_PER_DAY as u64 / 4 - 1 {
            night_hp.push(sim.hq().hp);
        }
    }
    format!(
        "graine {seed}, {towers} tours : {:?} au jour {}, PV du QG à chaque aube {:?}, {} ennemis tués",
        sim.outcome,
        sim.tick / TICKS_PER_DAY as u64 + 1,
        night_hp,
        sim.kills
    )
}

fn main() {
    for seed in [2026, 7, 99] {
        for towers in [0, 2, 4, 6] {
            println!("{}", play(seed, towers));
        }
    }
}
