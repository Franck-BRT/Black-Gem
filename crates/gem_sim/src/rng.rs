//! Générateurs pseudo-aléatoires déterministes.
//!
//! Tout le hasard de la simulation passe par ici : à graine identique, résultat
//! identique sur toutes les plateformes (Windows, Linux, macOS, WebAssembly).
//! C'est la condition pour le multijoueur en lockstep (LAN et en ligne) et les replays.

/// Mélange un entier 64 bits (finaliseur de SplitMix64).
pub const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hache une position de grille 2D avec une graine.
pub const fn hash2(seed: u64, x: i32, y: i32) -> u64 {
    mix64(seed ^ mix64((x as u32 as u64) | ((y as u32 as u64) << 32)))
}

/// Générateur SplitMix64 : rapide, simple, suffisant pour du gameplay.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }

    /// Entier uniforme dans `0..n` (`n` > 0).
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }

    /// Entier uniforme dans `lo..=hi`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as u32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_reproducible() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_stays_in_bounds() {
        let mut rng = Rng::new(7);
        for _ in 0..10_000 {
            assert!(rng.below(13) < 13);
            let v = rng.range(-5, 5);
            assert!((-5..=5).contains(&v));
        }
    }
}
