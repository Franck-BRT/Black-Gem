//! Équilibrage : joue automatiquement quelques stratégies et affiche le résultat.
//! `cargo run -p gem_sim --example balance`

use gem_sim::{
    BuildingKind, BuildingState, Command, Dir, Gem, GemColor, Simulation, TICKS_PER_DAY, Tile,
};

/// Des tours autour du QG, approvisionnées « par magie » (on teste la puissance de feu,
/// pas la logistique), avec éventuellement une gemme de chaque couleur au grade donné.
fn play(seed: u64, towers: usize, grade: u8) -> String {
    let mut sim = Simulation::new(seed, 192);
    let hq = sim.hq().tile;
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
    for (i, &(dx, dy)) in spots.iter().take(towers).enumerate() {
        let tile = Tile::new(hq.x + dx, hq.y + dy);
        sim.queue(Command::Build {
            kind: BuildingKind::Tower,
            tile,
            dir: Dir::East,
        });
        if grade > 0 {
            let gem = Gem::new(GemColor::ALL[i % 5], grade);
            sim.gems.push(gem);
            sim.queue(Command::Socket { tile, gem });
        }
    }
    sim.stock = 1000;
    let mut dawn_hp = Vec::new();
    while sim.outcome.is_none() && sim.tick < TICKS_PER_DAY as u64 * 4 {
        for id in sim.buildings.ids() {
            if let Some(b) = sim.buildings.get_mut(id)
                && let BuildingState::Tower { crystals, .. } = &mut b.state
            {
                *crystals = 4;
            }
        }
        sim.step();
        if sim.tick % TICKS_PER_DAY as u64 == TICKS_PER_DAY as u64 / 4 - 1 {
            dawn_hp.push(sim.hq().hp);
        }
    }
    format!(
        "graine {seed}, {towers} tours, gemmes de grade {grade} : {:?} au jour {}, PV du QG à chaque aube {:?}, {} ennemis tués",
        sim.outcome,
        sim.tick / TICKS_PER_DAY as u64 + 1,
        dawn_hp,
        sim.kills
    )
}

fn main() {
    for seed in [2026, 7, 99] {
        for (towers, grade) in [(4, 0), (6, 0), (8, 0), (4, 1), (4, 2), (4, 3), (6, 2)] {
            println!("{}", play(seed, towers, grade));
        }
    }
}
