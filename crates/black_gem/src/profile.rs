//! L'Arche : progression conservée d'une expédition à l'autre (éclats noirs, recherche,
//! codex des gemmes, bibliothèque de scripts, statistiques), et sa sauvegarde.
//!
//! Ordinateur : un fichier `profile.json` dans le dossier de données de l'utilisateur.
//! Navigateur (iPhone compris) : le stockage local du site.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use gem_sim::{
    Bonuses, BuildingState, Format, Gem, GemColor, Item, Outcome, Simulation, UNITS_PER_TILE,
};
use serde::{Deserialize, Serialize};

use crate::{
    i18n::Language,
    mission::playing,
    sim::{NewPlanet, Sim},
};

/// Un programme enregistré dans la bibliothèque.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedScript {
    pub name: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub expeditions: u32,
    pub victories: u32,
    pub kills: u32,
    /// Meilleur temps de victoire par format, en secondes de jeu.
    pub best_seconds: BTreeMap<String, u64>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Éclats noirs : la monnaie de l'Arche.
    pub shards: u32,
    /// Recherches acquises (identifiants).
    pub research: BTreeSet<String>,
    /// Gemmes découvertes : combinaison de couleurs → meilleur grade.
    pub codex: BTreeMap<String, u8>,
    pub scripts: Vec<SavedScript>,
    pub stats: Stats,
    /// Langue de l'interface (« fr » / « en »).
    pub language: Option<String>,
}

/// Une recherche de l'Arche : amélioration permanente.
pub struct Research {
    pub id: &'static str,
    pub name: (&'static str, &'static str),
    pub description: (&'static str, &'static str),
    pub cost: u32,
    pub requires: &'static [&'static str],
    apply: fn(&mut Bonuses),
}

impl Research {
    pub fn name(&self, lang: Language) -> &'static str {
        if lang == Language::French {
            self.name.0
        } else {
            self.name.1
        }
    }

    pub fn description(&self, lang: Language) -> &'static str {
        if lang == Language::French {
            self.description.0
        } else {
            self.description.1
        }
    }
}

pub const RESEARCH: [Research; 12] = [
    Research {
        id: "reserves",
        name: ("Réserves de départ", "Starting reserves"),
        description: ("+30 cristaux à l'atterrissage", "+30 crystals on landing"),
        cost: 10,
        requires: &[],
        apply: |b| b.starting_crystals += 30,
    },
    Research {
        id: "armor",
        name: ("Blindage du QG", "HQ armor"),
        description: (
            "+300 points de vie pour le QG",
            "+300 hit points for the HQ",
        ),
        cost: 15,
        requires: &[],
        apply: |b| b.hq_hp += 300,
    },
    Research {
        id: "golems",
        name: ("Golems véloces", "Swift golems"),
        description: ("Les golems volent 50 % plus vite", "Golems fly 50% faster"),
        cost: 15,
        requires: &[],
        apply: |b| b.golem_speed = b.golem_speed * 3 / 2,
    },
    Research {
        id: "drills",
        name: ("Foreuses rapides", "Fast drills"),
        description: (
            "Les foreuses produisent 33 % plus vite",
            "Drills produce 33% faster",
        ),
        cost: 20,
        requires: &[],
        apply: |b| b.drill_period = b.drill_period * 3 / 4,
    },
    Research {
        id: "towers",
        name: ("Tours renforcées", "Reinforced towers"),
        description: ("+25 % de dégâts pour les tours", "+25% tower damage"),
        cost: 25,
        requires: &[],
        apply: |b| b.tower_damage += 25,
    },
    Research {
        id: "big_reserves",
        name: ("Grandes réserves", "Large reserves"),
        description: (
            "+40 cristaux de plus à l'atterrissage",
            "+40 more crystals on landing",
        ),
        cost: 30,
        requires: &["reserves"],
        apply: |b| b.starting_crystals += 40,
    },
    Research {
        id: "belts",
        name: ("Convoyeurs rapides", "Fast conveyors"),
        description: (
            "Les convoyeurs vont 50 % plus vite",
            "Conveyors move 50% faster",
        ),
        cost: 20,
        requires: &["drills"],
        apply: |b| b.conveyor_ticks = b.conveyor_ticks * 2 / 3,
    },
    Research {
        id: "range",
        name: ("Optique de visée", "Targeting optics"),
        description: ("+1 case de portée pour les tours", "+1 tile of tower range"),
        cost: 25,
        requires: &["towers"],
        apply: |b| b.tower_range += UNITS_PER_TILE,
    },
    Research {
        id: "cheap_golems",
        name: ("Golems économiques", "Cheap golems"),
        description: (
            "Un golem coûte 8 cristaux au lieu de 15",
            "A golem costs 8 crystals instead of 15",
        ),
        cost: 25,
        requires: &["golems"],
        apply: |b| b.golem_cost = 8,
    },
    Research {
        id: "cutting",
        name: ("Taille efficace", "Efficient cutting"),
        description: (
            "2 cristaux suffisent pour tailler une gemme",
            "2 crystals are enough to cut a gem",
        ),
        cost: 35,
        requires: &["drills"],
        apply: |b| b.crystals_per_gem = 2,
    },
    Research {
        id: "core",
        name: ("Trépan du Cœur", "Core bit"),
        description: (
            "Le Grand foreur creuse 40 % plus vite",
            "The Great drill digs 40% faster",
        ),
        cost: 40,
        requires: &["drills"],
        apply: |b| b.core_drill_period = b.core_drill_period * 5 / 7,
    },
    Research {
        id: "gem",
        name: ("Gemme de départ", "Starting gem"),
        description: (
            "Une gemme de grade 2 offerte à l'atterrissage",
            "A grade 2 gem on landing",
        ),
        cost: 40,
        requires: &["cutting"],
        apply: |b| b.starting_gem = true,
    },
];

impl Profile {
    pub fn has(&self, id: &str) -> bool {
        self.research.contains(id)
    }

    pub fn can_research(&self, research: &Research) -> bool {
        !self.has(research.id)
            && research.requires.iter().all(|r| self.has(r))
            && self.shards >= research.cost
    }

    pub fn buy(&mut self, id: &str) -> bool {
        let Some(research) = RESEARCH.iter().find(|r| r.id == id) else {
            return false;
        };
        if !self.can_research(research) {
            return false;
        }
        self.shards -= research.cost;
        self.research.insert(id.to_string());
        true
    }

    /// Règles de la prochaine expédition, avec toutes les recherches acquises.
    pub fn bonuses(&self) -> Bonuses {
        let mut bonuses = Bonuses::default();
        for research in RESEARCH.iter().filter(|r| self.has(r.id)) {
            (research.apply)(&mut bonuses);
        }
        bonuses
    }

    /// Enregistre une gemme dans le codex. Renvoie `true` si c'est une découverte.
    pub fn discover(&mut self, gem: Gem) -> bool {
        let entry = self.codex.entry(codex_key(gem)).or_insert(0);
        let new = *entry == 0;
        *entry = (*entry).max(gem.grade);
        new
    }

    /// Enregistre un programme (remplace celui du même nom).
    pub fn save_script(&mut self, source: &str) -> String {
        let name = script_name(source, self.scripts.len() + 1);
        match self.scripts.iter_mut().find(|s| s.name == name) {
            Some(existing) => existing.source = source.to_string(),
            None => self.scripts.push(SavedScript {
                name: name.clone(),
                source: source.to_string(),
            }),
        }
        name
    }
}

/// Nom d'un programme : le premier mot de son commentaire d'en-tête, sinon « Programme N ».
fn script_name(source: &str, n: usize) -> String {
    source
        .lines()
        .next()
        .and_then(|line| line.trim().strip_prefix('#'))
        .and_then(|comment| comment.split([':', '-', '.']).next())
        .map(|name| name.trim().chars().take(20).collect::<String>())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("Programme {n}"))
}

/// Clé du codex : les couleurs de la gemme, dans l'ordre (« Red+Yellow »).
pub fn codex_key(gem: Gem) -> String {
    gem.colors()
        .map(|c| format!("{c:?}"))
        .collect::<Vec<_>>()
        .join("+")
}

/// Toutes les combinaisons de couleurs possibles (31), dans l'ordre d'affichage.
pub fn all_color_sets() -> Vec<Vec<GemColor>> {
    let mut sets: Vec<Vec<GemColor>> = (1u8..32)
        .map(|mask| {
            GemColor::ALL
                .into_iter()
                .filter(|c| mask & (1 << c.index()) != 0)
                .collect()
        })
        .collect();
    sets.sort_by_key(|set| (set.len(), set.iter().map(|c| c.index()).collect::<Vec<_>>()));
    sets
}

/// Éclats noirs gagnés à la fin d'une expédition.
pub fn reward(sim: &Simulation) -> u32 {
    let format = sim.config.format;
    let nights = sim.wave;
    match (format, sim.outcome) {
        (Format::Colony, _) => nights * 5,
        (_, Some(Outcome::Victory)) => {
            let base = match format {
                Format::Scout => 15,
                Format::Expedition => 40,
                Format::GreatDrill => 90,
                Format::Colony => 0,
            };
            base + nights * 2
        }
        _ => sim.stage * 5 + nights * 2,
    }
}

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        let profile = load();
        app.insert_resource(profile)
            .init_resource::<LastReward>()
            .add_systems(Startup, apply_saved_language)
            .add_systems(
                Update,
                (
                    record_codex.run_if(playing),
                    settle_expedition,
                    reset_reward.run_if(on_message::<NewPlanet>),
                    remember_language,
                    save_when_changed,
                )
                    .chain(),
            );
    }
}

/// Récompense de l'expédition qui vient de se terminer (affichée sur l'écran de fin).
#[derive(Resource, Default)]
pub struct LastReward(pub Option<u32>);

fn reset_reward(mut reward: ResMut<LastReward>) {
    reward.0 = None;
}

fn apply_saved_language(profile: Res<Profile>, mut language: ResMut<Language>) {
    match profile.language.as_deref() {
        Some("en") => *language = Language::English,
        Some("fr") => *language = Language::French,
        _ => {}
    }
}

fn remember_language(language: Res<Language>, mut profile: ResMut<Profile>) {
    if language.is_changed() {
        let code = if *language == Language::French {
            "fr"
        } else {
            "en"
        };
        if profile.language.as_deref() != Some(code) {
            profile.language = Some(code.to_string());
        }
    }
}

/// Relève les gemmes présentes (inventaire, tours, fours, convoyeurs, golems) pour le codex.
fn record_codex(
    sim: Res<Sim>,
    mut profile: ResMut<Profile>,
    time: Res<Time<Real>>,
    mut last: Local<f32>,
) {
    if time.elapsed_secs() - *last < 1.0 {
        return;
    }
    *last = time.elapsed_secs();
    let mut gems: Vec<Gem> = sim.gems.clone();
    for building in sim.buildings.iter() {
        match &building.state {
            BuildingState::Tower { gem: Some(gem), .. } => gems.push(*gem),
            BuildingState::Furnace { slots, output, .. } => {
                gems.extend(slots.iter().flatten());
                gems.extend(output);
            }
            BuildingState::Cutter { output, .. } => gems.extend(output),
            BuildingState::Conveyor {
                item: Some((Item::Gem(gem), _)),
            } => gems.push(*gem),
            _ => {}
        }
    }
    gems.extend(sim.golems.iter().filter_map(|g| match g.carrying {
        Some(Item::Gem(gem)) => Some(gem),
        _ => None,
    }));
    // Ne touche au profil (et donc à la sauvegarde) que s'il y a du nouveau.
    let changes = gems.iter().any(|gem| {
        profile
            .codex
            .get(&codex_key(*gem))
            .is_none_or(|best| *best < gem.grade)
    });
    if changes {
        for gem in gems {
            profile.discover(gem);
        }
    }
}

/// Fin d'expédition : récompense, statistiques, sauvegarde (une seule fois par expédition).
fn settle_expedition(
    sim: Res<Sim>,
    mut profile: ResMut<Profile>,
    mut last_reward: ResMut<LastReward>,
    mut new_planets: MessageReader<NewPlanet>,
    mut settled: Local<bool>,
) {
    // Chaque nouvelle expédition pourra être récompensée (même sur la même planète).
    if new_planets.read().count() > 0 {
        *settled = false;
    }
    let Some(outcome) = sim.outcome else {
        return;
    };
    if *settled {
        return;
    }
    *settled = true;
    let earned = reward(&sim);
    last_reward.0 = Some(earned);
    profile.shards += earned;
    profile.stats.expeditions += 1;
    profile.stats.kills += sim.kills;
    if outcome == Outcome::Victory {
        profile.stats.victories += 1;
        let seconds =
            (sim.tick - sim.config.ticks_per_day as u64 / 4) / gem_sim::TICKS_PER_SECOND as u64;
        let best = profile
            .stats
            .best_seconds
            .entry(format!("{:?}", sim.config.format))
            .or_insert(u64::MAX);
        *best = (*best).min(seconds);
    }
}

fn save_when_changed(profile: Res<Profile>) {
    if profile.is_changed() && !profile.is_added() {
        save(&profile);
    }
}

// --- Stockage -------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn profile_path() -> Option<std::path::PathBuf> {
    use std::{env, path::PathBuf};
    let base = if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("BlackGem").join("profile.json"))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_raw() -> Option<String> {
    std::fs::read_to_string(profile_path()?).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write_raw(text: &str) {
    let Some(path) = profile_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Écriture atomique : un fichier temporaire, puis renommage.
    let temporary = path.with_extension("json.tmp");
    if std::fs::write(&temporary, text).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    }
}

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "black_gem_profile";

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn read_raw() -> Option<String> {
    local_storage()?.get_item(STORAGE_KEY).ok()?
}

#[cfg(target_arch = "wasm32")]
fn write_raw(text: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(STORAGE_KEY, text);
    }
}

fn load() -> Profile {
    read_raw()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(profile: &Profile) {
    if let Ok(text) = serde_json::to_string_pretty(profile) {
        write_raw(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_requires_prerequisites_and_shards() {
        let mut profile = Profile {
            shards: 100,
            ..Profile::default()
        };
        assert!(
            !profile.buy("belts"),
            "il faut d'abord les foreuses rapides"
        );
        assert!(profile.buy("drills"));
        assert!(profile.buy("belts"));
        assert!(!profile.buy("belts"), "déjà acquise");
        assert_eq!(profile.shards, 60);
        let bonuses = profile.bonuses();
        assert_eq!(
            bonuses.drill_period,
            Bonuses::default().drill_period * 3 / 4
        );
        assert_eq!(bonuses.conveyor_ticks, 10);
        profile.shards = 5;
        assert!(!profile.buy("armor"));
    }

    #[test]
    fn every_prerequisite_exists() {
        for research in &RESEARCH {
            for required in research.requires {
                assert!(RESEARCH.iter().any(|r| r.id == *required), "{required}");
            }
        }
    }

    #[test]
    fn codex_keeps_the_best_grade() {
        let mut profile = Profile::default();
        let hybrid = Gem::new(GemColor::Red, 1)
            .fuse(Gem::new(GemColor::Yellow, 1))
            .unwrap();
        assert!(profile.discover(hybrid));
        assert!(!profile.discover(hybrid));
        assert!(profile.discover(Gem::new(GemColor::Cyan, 3)));
        assert_eq!(profile.codex.get("Red+Yellow"), Some(&2));
        assert_eq!(all_color_sets().len(), 31);
    }

    #[test]
    fn scripts_are_named_after_their_header() {
        let mut profile = Profile::default();
        assert_eq!(
            profile.save_script("# Ravitailleur : apporte..."),
            "Ravitailleur"
        );
        assert_eq!(profile.save_script("x = 1"), "Programme 2");
        assert_eq!(
            profile.save_script("# Ravitailleur v2 - mieux"),
            "Ravitailleur v2"
        );
        assert_eq!(profile.scripts.len(), 3);
        profile.save_script("# Ravitailleur : nouvelle version");
        assert_eq!(profile.scripts.len(), 3);
    }

    #[test]
    fn profile_round_trips_through_json() {
        let mut profile = Profile {
            shards: 42,
            ..Profile::default()
        };
        profile.research.insert("armor".into());
        profile.save_script("# Test\nprint(1)");
        let text = serde_json::to_string(&profile).unwrap();
        assert_eq!(serde_json::from_str::<Profile>(&text).unwrap(), profile);
        // Une sauvegarde plus ancienne (champs manquants) se charge quand même.
        let old: Profile = serde_json::from_str(r#"{"shards": 3}"#).unwrap();
        assert_eq!(old.shards, 3);
    }

    #[test]
    fn rewards_depend_on_the_format_and_outcome() {
        let mut sim = Simulation::with_format(1, Format::Expedition);
        sim.wave = 3;
        sim.outcome = Some(Outcome::Victory);
        assert_eq!(reward(&sim), 46);
        sim.outcome = Some(Outcome::Defeat);
        sim.stage = 2;
        assert_eq!(reward(&sim), 16);
    }
}
