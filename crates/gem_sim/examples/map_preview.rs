//! Aperçu ASCII d'une carte : `cargo run -p gem_sim --example map_preview -- <graine>`

use gem_sim::{Map, Terrain};

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2026);
    let map = Map::generate(seed, 128);
    for y in (0..map.size).step_by(2) {
        let line: String = (0..map.size)
            .map(|x| match map.tile(x, y) {
                Terrain::Plain => '.',
                Terrain::Rock => '^',
                Terrain::Water => '~',
                Terrain::CrystalVein => '*',
                Terrain::Ore => 'o',
            })
            .collect();
        println!("{line}");
    }
    for kind in Terrain::ALL {
        let count = map.tiles().filter(|&(_, _, t)| t == kind).count();
        println!(
            "{kind:?}: {:.1} %",
            count as f64 * 100.0 / (map.size * map.size) as f64
        );
    }
    println!("checksum: {}", map.checksum());
}
