//! Bruit de valeur en virgule fixe (entiers uniquement).
//!
//! Pas de flottants ici : `sin`, `exp`, `powf`… ne donnent pas exactement le même
//! résultat selon le processeur ou le navigateur. Avec des entiers, une graine
//! donne la même planète partout.

use crate::rng::hash2;

/// Valeur maximale (exclue) renvoyée par les fonctions de bruit.
pub const NOISE_ONE: i64 = 1 << 16;

/// Courbe d'atténuation `3t² - 2t³` en virgule fixe 16 bits.
const fn fade(t: i64) -> i64 {
    // t, résultat dans [0, NOISE_ONE]
    t * t / NOISE_ONE * (3 * NOISE_ONE - 2 * t) / NOISE_ONE
}

const fn lerp(a: i64, b: i64, t: i64) -> i64 {
    a + (b - a) * t / NOISE_ONE
}

const fn lattice(seed: u64, x: i32, y: i32) -> i64 {
    (hash2(seed, x, y) >> 48) as i64
}

/// Bruit de valeur lissé, une octave, de période `period` cases. Résultat dans `[0, NOISE_ONE)`.
pub fn value_noise(seed: u64, x: i32, y: i32, period: i32) -> i64 {
    let cx = x.div_euclid(period);
    let cy = y.div_euclid(period);
    let fx = fade(x.rem_euclid(period) as i64 * NOISE_ONE / period as i64);
    let fy = fade(y.rem_euclid(period) as i64 * NOISE_ONE / period as i64);

    let top = lerp(lattice(seed, cx, cy), lattice(seed, cx + 1, cy), fx);
    let bottom = lerp(lattice(seed, cx, cy + 1), lattice(seed, cx + 1, cy + 1), fx);
    lerp(top, bottom, fy)
}

/// Somme de plusieurs octaves (« fBm ») : grandes formes + détails. Résultat dans `[0, NOISE_ONE)`.
pub fn fbm(seed: u64, x: i32, y: i32, periods: &[i32]) -> i64 {
    let mut total = 0;
    let mut weight_sum = 0;
    let mut weight = 1 << periods.len();
    for (octave, &period) in periods.iter().enumerate() {
        let octave_seed =
            seed.wrapping_add(0x5851_F42D_4C95_7F2D_u64.wrapping_mul(octave as u64 + 1));
        total += value_noise(octave_seed, x, y, period) * weight;
        weight_sum += weight;
        weight /= 2;
    }
    total / weight_sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range() {
        for y in -50..50 {
            for x in -50..50 {
                let v = fbm(3, x, y, &[32, 16, 8, 4]);
                assert!((0..NOISE_ONE).contains(&v), "{v}");
            }
        }
    }

    #[test]
    fn noise_matches_lattice_on_grid_points() {
        assert_eq!(value_noise(9, 32, 64, 32), lattice(9, 1, 2));
    }
}
