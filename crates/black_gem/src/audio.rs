//! Sons et musique, **synthétisés au démarrage** : aucun fichier à télécharger, et le jeu
//! reste un seul exécutable.
//!
//! - effets : tirs (selon la gemme), destruction des ennemis, boss, constructions, coups
//!   sur les bâtiments, alarme de la nuit, victoire, défaite ;
//! - musique : une nappe calme le jour, une plus sombre et pulsée la nuit, en fondu.
//!
//! Touche M : couper / remettre le son (conservé dans le profil).

use std::{collections::HashMap, f32::consts::TAU, sync::Arc};

use bevy::{
    audio::{AudioSink, AudioSinkPlayback, PlaybackMode, Volume},
    prelude::*,
};
use gem_sim::{GemColor, SimEvent};

use crate::{
    editor::typing,
    mission::MissionMenu,
    profile::Profile,
    sim::{Sim, SimEvents},
};

const SFX_RATE: u32 = 22_050;
const MUSIC_RATE: u32 = 16_000;
/// Durée d'une boucle de musique, en secondes.
const LOOP_SECONDS: f32 = 32.0;
/// Sons joués en même temps, au plus.
const MAX_VOICES: usize = 14;
const MUSIC_VOLUME: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Sfx {
    /// Tir de tour : la hauteur dépend de la couleur de la gemme.
    Shot(u8),
    Kill,
    BossKill,
    Build,
    Demolish,
    Destroyed,
    HqHit,
    Hit,
    Night,
    Stage,
    Golem,
    Victory,
    Defeat,
    /// Unité formée : clairon bref.
    Trained,
    /// Pouvoir du héros : souffle cristallin.
    Ability,
    /// Le héros monte de niveau.
    LevelUp,
}

impl Sfx {
    const ALL: [Sfx; 21] = [
        Sfx::Shot(0),
        Sfx::Shot(1),
        Sfx::Shot(2),
        Sfx::Shot(3),
        Sfx::Shot(4),
        Sfx::Shot(5),
        Sfx::Kill,
        Sfx::BossKill,
        Sfx::Build,
        Sfx::Demolish,
        Sfx::Destroyed,
        Sfx::HqHit,
        Sfx::Hit,
        Sfx::Night,
        Sfx::Stage,
        Sfx::Golem,
        Sfx::Victory,
        Sfx::Defeat,
        Sfx::Trained,
        Sfx::Ability,
        Sfx::LevelUp,
    ];

    /// Intervalle minimal entre deux lectures du même son (secondes), pour ne pas saturer
    /// quand vingt tours tirent ensemble.
    fn spacing(self) -> f32 {
        match self {
            Sfx::Shot(_) => 0.07,
            Sfx::Kill | Sfx::Hit => 0.09,
            Sfx::HqHit => 0.35,
            _ => 0.0,
        }
    }

    fn volume(self) -> f32 {
        match self {
            Sfx::Shot(_) => 0.35,
            Sfx::Hit => 0.45,
            Sfx::Kill => 0.6,
            _ => 0.8,
        }
    }
}

#[derive(Resource)]
struct SoundBank {
    sfx: HashMap<Sfx, Handle<AudioSource>>,
}

#[derive(Component)]
struct Voice;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Music {
    Day,
    Night,
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (play_events, mix_music, toggle_mute.run_if(not(typing))),
        );
    }
}

fn setup(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut sfx = HashMap::new();
    for kind in Sfx::ALL {
        let samples = synth_sfx(kind);
        sfx.insert(kind, sources.add(source(&samples, SFX_RATE)));
    }
    commands.insert_resource(SoundBank { sfx });
    for (music, samples) in [(Music::Day, day_music()), (Music::Night, night_music())] {
        commands.spawn((
            music,
            AudioPlayer::new(sources.add(source(&samples, MUSIC_RATE))),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(0.0),
                ..PlaybackSettings::LOOP
            },
        ));
    }
}

fn toggle_mute(keys: Res<ButtonInput<KeyCode>>, mut profile: ResMut<Profile>) {
    if keys.just_pressed(KeyCode::KeyM) {
        profile.muted = !profile.muted;
    }
}

fn sfx_for(event: &SimEvent) -> Option<Sfx> {
    Some(match event {
        SimEvent::Shot { color } => Sfx::Shot(color.map_or(5, |c: GemColor| c.index() as u8)),
        SimEvent::EnemyKilled { boss: true, .. } => Sfx::BossKill,
        SimEvent::EnemyKilled { .. } => Sfx::Kill,
        SimEvent::Built { .. } => Sfx::Build,
        SimEvent::Demolished { .. } => Sfx::Demolish,
        SimEvent::BuildingDestroyed { .. } => Sfx::Destroyed,
        SimEvent::BuildingHit { .. } => Sfx::Hit,
        SimEvent::HqHit => Sfx::HqHit,
        SimEvent::NightFalls { .. } => Sfx::Night,
        SimEvent::StageReached { .. } => Sfx::Stage,
        SimEvent::GolemBuilt => Sfx::Golem,
        SimEvent::Victory => Sfx::Victory,
        SimEvent::Defeat => Sfx::Defeat,
        SimEvent::UnitTrained => Sfx::Trained,
        SimEvent::UnitLost => Sfx::Hit,
        SimEvent::NestDestroyed { .. } => Sfx::BossKill,
        SimEvent::HeroLevelUp { .. } => Sfx::LevelUp,
        SimEvent::AbilityUsed { .. } => Sfx::Ability,
        SimEvent::PowerShortage => return None,
    })
}

fn play_events(
    mut commands: Commands,
    events: Res<SimEvents>,
    bank: Res<SoundBank>,
    profile: Res<Profile>,
    time: Res<Time<Real>>,
    voices: Query<(), With<Voice>>,
    mut last_played: Local<HashMap<Sfx, f32>>,
) {
    if profile.muted {
        return;
    }
    let now = time.elapsed_secs();
    let mut playing = voices.iter().count();
    for kind in events.iter().filter_map(sfx_for) {
        if playing >= MAX_VOICES {
            break;
        }
        let last = last_played.get(&kind).copied().unwrap_or(f32::MIN);
        if now - last < kind.spacing() {
            continue;
        }
        last_played.insert(kind, now);
        playing += 1;
        commands.spawn((
            Voice,
            AudioPlayer::new(bank.sfx[&kind].clone()),
            PlaybackSettings {
                volume: Volume::Linear(kind.volume()),
                ..PlaybackSettings::DESPAWN
            },
        ));
    }
}

/// Fondu entre la musique de jour et celle de nuit.
fn mix_music(
    sim: Res<Sim>,
    menu: Res<MissionMenu>,
    profile: Res<Profile>,
    time: Res<Time<Real>>,
    mut sinks: Query<(&Music, &mut AudioSink)>,
) {
    let night = !menu.open && sim.is_night() && sim.outcome.is_none();
    let rate = (time.delta_secs() * 0.4).min(1.0);
    for (music, mut sink) in &mut sinks {
        let wanted = match (profile.muted, music, night) {
            (true, _, _) => 0.0,
            (false, Music::Day, false) | (false, Music::Night, true) => MUSIC_VOLUME,
            _ => 0.0,
        };
        let current = sink.volume().to_linear();
        let next = if profile.muted {
            0.0
        } else {
            current + (wanted - current) * rate.max(0.02)
        };
        if (next - current).abs() > 1e-4 || (next == 0.0 && current != 0.0) {
            sink.set_volume(Volume::Linear(next));
        }
    }
}

// ----- Synthèse -----

/// Fichier WAV (16 bits, mono) en mémoire.
fn source(samples: &[f32], rate: u32) -> AudioSource {
    let data_len = samples.len() as u32 * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    AudioSource {
        bytes: Arc::from(bytes),
    }
}

/// Bruit déterministe.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Filtre passe-bas d'ordre 1.
struct LowPass {
    value: f32,
    alpha: f32,
}

impl LowPass {
    fn new(cutoff: f32, rate: u32) -> Self {
        let dt = 1.0 / rate as f32;
        let rc = 1.0 / (TAU * cutoff);
        Self {
            value: 0.0,
            alpha: dt / (rc + dt),
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        self.value += self.alpha * (x - self.value);
        self.value
    }
}

/// Fabrique `seconds` d'échantillons ; `f(t, phase)` reçoit le temps et une phase intégrée
/// pour la fréquence renvoyée par `freq(t)` (balayages sans craquement).
fn render(seconds: f32, freq: impl Fn(f32) -> f32, mut f: impl FnMut(f32, f32) -> f32) -> Vec<f32> {
    let n = (seconds * SFX_RATE as f32) as usize;
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SFX_RATE as f32;
        phase = (phase + freq(t) / SFX_RATE as f32).fract();
        out.push(f(t, phase));
    }
    // Pas de clic à la fin.
    let fade = (SFX_RATE as usize / 200).min(out.len());
    let len = out.len();
    for (k, s) in out[len - fade..].iter_mut().enumerate() {
        *s *= 1.0 - k as f32 / fade as f32;
    }
    out
}

fn sine(phase: f32) -> f32 {
    (phase * TAU).sin()
}

fn triangle(phase: f32) -> f32 {
    4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0
}

/// Note d'une mélodie : fréquence, début, durée.
fn notes(list: &[(f32, f32, f32)], total: f32, bell: bool) -> Vec<f32> {
    let n = (total * SFX_RATE as f32) as usize;
    let mut out = vec![0.0; n];
    for &(freq, start, length) in list {
        let first = (start * SFX_RATE as f32) as usize;
        let count = (length * SFX_RATE as f32) as usize;
        for k in 0..count.min(n.saturating_sub(first)) {
            let t = k as f32 / SFX_RATE as f32;
            let attack = (t / 0.01).min(1.0);
            let env = if bell {
                (-t * 4.0).exp()
            } else {
                (1.0 - t / length).max(0.0)
            };
            let p = freq * t;
            let wave = if bell {
                sine(p) + 0.3 * sine(p * 2.0) + 0.15 * sine(p * 3.01)
            } else {
                triangle(p) * 0.8 + 0.2 * sine(p * 0.5)
            };
            out[first + k] += wave * env * attack * 0.5;
        }
    }
    out
}

/// Un effet, ramené à un niveau commun (le volume relatif est réglé à la lecture).
fn synth_sfx(kind: Sfx) -> Vec<f32> {
    let mut samples = synth_raw(kind);
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        let gain = 0.85 / peak;
        samples.iter_mut().for_each(|s| *s *= gain);
    }
    samples
}

fn synth_raw(kind: Sfx) -> Vec<f32> {
    let mut noise = Noise(0x1234_5678);
    match kind {
        Sfx::Shot(variant) => {
            // Rouge grave… violet aigu ; sans gemme, un petit claquement neutre.
            let base = [700.0, 1100.0, 900.0, 1300.0, 1600.0, 1000.0][variant as usize % 6];
            render(
                0.1,
                |t| base * (1.0 - t * 5.5).max(0.35),
                |t, p| {
                    let env = (-t * 38.0).exp();
                    (sine(p) * 0.7 + 0.3 * sine(p * 3.0)) * env * 0.6
                },
            )
        }
        Sfx::Kill => {
            let mut lp = LowPass::new(1800.0, SFX_RATE);
            render(
                0.22,
                |t| 220.0 * (1.0 - t * 3.0).max(0.25),
                |t, p| {
                    let env = (-t * 18.0).exp();
                    (lp.run(noise.next()) * 0.8 + sine(p) * 0.5) * env
                },
            )
        }
        Sfx::BossKill => {
            let mut lp = LowPass::new(500.0, SFX_RATE);
            render(
                1.1,
                |t| 95.0 * (1.0 - t * 0.7).max(0.3),
                |t, p| {
                    let env = (-t * 3.5).exp();
                    (lp.run(noise.next()) * 1.4 + sine(p) * 0.8) * env * 0.8
                },
            )
        }
        Sfx::Build => {
            let tone = notes(&[(520.0, 0.0, 0.07), (780.0, 0.06, 0.1)], 0.18, false);
            let mut lp = LowPass::new(900.0, SFX_RATE);
            tone.iter()
                .enumerate()
                .map(|(i, s)| {
                    let t = i as f32 / SFX_RATE as f32;
                    s + lp.run(noise.next()) * (-t * 60.0).exp() * 0.6
                })
                .collect()
        }
        Sfx::Demolish => {
            let mut lp = LowPass::new(1200.0, SFX_RATE);
            render(
                0.3,
                |t| 300.0 * (1.0 - t * 2.0).max(0.4),
                |t, p| (lp.run(noise.next()) * 0.7 + triangle(p) * 0.4) * (-t * 10.0).exp(),
            )
        }
        Sfx::Destroyed => {
            let mut lp = LowPass::new(900.0, SFX_RATE);
            render(
                0.6,
                |t| 200.0 * (1.0 - t * 1.4).max(0.25),
                |t, p| (lp.run(noise.next()) * 1.2 + sine(p) * 0.6) * (-t * 6.0).exp() * 0.8,
            )
        }
        Sfx::HqHit => render(
            0.35,
            |t| 120.0 * (1.0 - t * 2.0).max(0.45),
            |t, p| {
                let click = if t < 0.006 { noise.next() * 0.6 } else { 0.0 };
                (sine(p) * (-t * 11.0).exp()) * 0.9 + click
            },
        ),
        Sfx::Hit => {
            let mut lp = LowPass::new(700.0, SFX_RATE);
            render(
                0.09,
                |_| 160.0,
                |t, p| (lp.run(noise.next()) * 1.2 + sine(p) * 0.3) * (-t * 45.0).exp(),
            )
        }
        Sfx::Night => {
            // Cor d'alarme : deux notes qui descendent, attaque lente.
            render(
                1.8,
                |t| if t < 0.8 { 220.0 } else { 185.0 },
                |t, p| {
                    let env = (t / 0.12).min(1.0) * (1.0 - (t - 1.4).max(0.0) / 0.4).max(0.0);
                    let saw = 2.0 * p - 1.0;
                    (saw * 0.35 + triangle(p) * 0.5 + 0.2 * sine(p * 0.5)) * env * 0.5
                },
            )
        }
        Sfx::Stage => render(
            1.4,
            |_| 110.0,
            |t, p| {
                let env = (t / 0.2).min(1.0) * (1.0 - t / 1.4);
                let tremolo = 0.75 + 0.25 * (t * TAU * 6.0).sin();
                (sine(p) + 0.6 * sine(p * 1.5) + 0.5 * sine(p * 2.0)) * env * tremolo * 0.35
            },
        ),
        Sfx::Golem => render(
            0.22,
            |t| 600.0 + 3800.0 * t,
            |t, p| (sine(p) * 0.7 + 0.3 * sine(p * 2.0)) * (1.0 - t / 0.22) * 0.5,
        ),
        Sfx::Victory => notes(
            &[
                (523.3, 0.0, 0.4),
                (659.3, 0.15, 0.4),
                (784.0, 0.3, 0.4),
                (1046.5, 0.45, 1.2),
            ],
            1.7,
            true,
        ),
        Sfx::Defeat => notes(
            &[
                (440.0, 0.0, 0.35),
                (392.0, 0.32, 0.35),
                (349.2, 0.64, 0.35),
                (329.6, 0.96, 1.0),
            ],
            2.0,
            false,
        ),
        Sfx::Trained => notes(&[(392.0, 0.0, 0.09), (587.3, 0.08, 0.16)], 0.26, false),
        Sfx::Ability => {
            let mut lp = LowPass::new(3000.0, SFX_RATE);
            render(
                0.8,
                |t| 1800.0 - 1300.0 * t,
                |t, p| {
                    let env = (t / 0.03).min(1.0) * (-t * 4.5).exp();
                    (sine(p) * 0.5 + 0.25 * sine(p * 2.01) + lp.run(noise.next()) * 0.6) * env
                },
            )
        }
        Sfx::LevelUp => notes(
            &[(659.3, 0.0, 0.3), (880.0, 0.1, 0.3), (1318.5, 0.2, 0.6)],
            0.9,
            true,
        ),
    }
}

/// Fréquence arrondie pour que la boucle se referme sans craquement.
fn looped(freq: f32) -> f32 {
    (freq * LOOP_SECONDS).round() / LOOP_SECONDS
}

/// Nappe d'accords : quatre accords de 8 s, en fondu enchaîné.
fn pad(chords: &[[f32; 3]; 4], brightness: f32) -> Vec<f32> {
    let n = (LOOP_SECONDS * MUSIC_RATE as f32) as usize;
    let step = LOOP_SECONDS / 4.0;
    let mut out = vec![0.0f32; n];
    let half = (step * 0.75 * MUSIC_RATE as f32) as i64;
    for (c, chord) in chords.iter().enumerate() {
        let center = c as f32 * step + step / 2.0;
        let center_sample = (center * MUSIC_RATE as f32) as i64;
        for &freq in chord {
            for (detune, gain) in [
                (1.0, 0.5),
                (1.004, 0.35),
                (0.5, 0.25),
                (2.0, 0.12 * brightness),
            ] {
                let f = looped(freq * detune);
                for offset in -half..half {
                    // Échantillon dans la boucle (elle se referme sur elle-même).
                    let i = (center_sample + offset).rem_euclid(n as i64) as usize;
                    let t = i as f32 / MUSIC_RATE as f32;
                    let d = offset.unsigned_abs() as f32 / MUSIC_RATE as f32;
                    let window = (std::f32::consts::PI * d / (step * 1.5)).cos().powi(2);
                    out[i] += (TAU * f * t).sin() * gain * window * 0.09;
                }
            }
        }
    }
    out
}

/// Notes de cloche douces, réparties sur la boucle.
fn add_bells(out: &mut [f32], scale: &[f32], every: f32, seed: u32) {
    let mut noise = Noise(seed);
    let mut t0 = 1.0;
    while t0 < LOOP_SECONDS - 3.0 {
        let note = scale[((noise.next() * 0.5 + 0.5) * scale.len() as f32) as usize % scale.len()];
        let f = looped(note);
        let start = (t0 * MUSIC_RATE as f32) as usize;
        for k in 0..(3.0 * MUSIC_RATE as f32) as usize {
            let t = k as f32 / MUSIC_RATE as f32;
            let env = (-t * 1.6).exp() * (t / 0.02).min(1.0);
            let v = ((TAU * f * t).sin() + 0.25 * (TAU * 2.0 * f * t).sin()) * env * 0.05;
            if let Some(s) = out.get_mut(start + k) {
                *s += v;
            }
        }
        t0 += every * (0.75 + 0.5 * (noise.next() * 0.5 + 0.5));
    }
}

fn day_music() -> Vec<f32> {
    // La mineur, Fa, Do, Sol : calme, ouvert.
    let mut out = pad(
        &[
            [220.0, 261.6, 329.6],
            [174.6, 220.0, 261.6],
            [261.6, 329.6, 392.0],
            [196.0, 246.9, 293.7],
        ],
        1.0,
    );
    add_bells(&mut out, &[659.3, 784.0, 880.0, 987.8, 1174.7], 2.2, 7);
    out
}

fn night_music() -> Vec<f32> {
    // Ré mineur, Si bémol, Sol mineur, La : plus grave, avec un battement sourd.
    let mut out = pad(
        &[
            [146.8, 174.6, 220.0],
            [116.5, 146.8, 174.6],
            [98.0, 116.5, 146.8],
            [110.0, 138.6, 164.8],
        ],
        0.4,
    );
    let beat = 60.0 / 75.0;
    let drone = looped(55.0);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / MUSIC_RATE as f32;
        let k = t % beat;
        let kick = (TAU * (55.0 * k - 40.0 * k * k)).sin() * (-k * 9.0).exp() * 0.35;
        let throb = (TAU * drone * t).sin() * (0.5 + 0.5 * (TAU * t / beat).sin()) * 0.06;
        *s += kick + throb;
    }
    add_bells(&mut out, &[440.0, 523.3, 587.3, 698.5], 4.0, 11);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn sounds_are_audible_and_do_not_clip() {
        for kind in Sfx::ALL {
            let samples = synth_sfx(kind);
            let p = peak(&samples);
            assert!(p > 0.05 && p <= 1.0, "{kind:?} : crête {p}");
            assert!(samples.len() < SFX_RATE as usize * 3, "{kind:?} trop long");
        }
    }

    #[test]
    fn music_loops_without_clicks() {
        for (name, music) in [("jour", day_music()), ("nuit", night_music())] {
            let p = peak(&music);
            assert!(p > 0.1 && p < 1.0, "{name} : crête {p}");
            let (first, last) = (music[0], *music.last().unwrap());
            assert!(
                (first - last).abs() < 0.05,
                "{name} : saut {first} → {last}"
            );
        }
    }

    #[test]
    fn wav_header() {
        let wav = source(&[0.0, 0.5, -0.5], 8000);
        assert_eq!(&wav.bytes[0..4], b"RIFF");
        assert_eq!(wav.bytes.len(), 44 + 6);
    }
}

/// Fichier d'écoute : tous les effets, puis 12 s de chaque musique.
/// `BLACK_GEM_DEMO=chemin.wav cargo test -p black_gem export_demo -- --ignored`
#[cfg(test)]
#[test]
#[ignore]
fn export_demo() {
    let path = std::env::var("BLACK_GEM_DEMO").expect("BLACK_GEM_DEMO");
    let mut all = Vec::new();
    let silence = |seconds: f32| vec![0.0f32; (seconds * SFX_RATE as f32) as usize];
    for kind in Sfx::ALL {
        all.extend(synth_sfx(kind).iter().map(|s| s * kind.volume()));
        all.extend(silence(0.35));
    }
    for music in [day_music(), night_music()] {
        // Rééchantillonnage simple 16 kHz → 22,05 kHz.
        let n = (12.0 * SFX_RATE as f32) as usize;
        for i in 0..n {
            let t = i as f32 * MUSIC_RATE as f32 / SFX_RATE as f32;
            all.push(music[t as usize % music.len()] * MUSIC_VOLUME * 2.0);
        }
        all.extend(silence(0.5));
    }
    std::fs::write(path, &*source(&all, SFX_RATE).bytes).unwrap();
}
