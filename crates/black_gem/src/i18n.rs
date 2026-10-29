//! Textes de l'interface en français et en anglais. Touche [L] pour changer de langue.

use bevy::prelude::*;
use gem_sim::{BuildError, BuildingKind, Gem, GemColor};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    French,
    English,
}

/// Identifiant d'un texte de l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tr {
    Subtitle,
    Seed,
    Day,
    DayTime,
    NightTime,
    Speed,
    Paused,
    Controls,
    LanguageName,
    Crystals,
    HqHealth,
    Wave,
    NightIn,
    NightNow,
    Kills,
    Demolish,
    Victory,
    VictoryDetail,
    Defeat,
    DefeatDetail,
    Unsocket,
    NeedsTower,
    Gems,
    NoGems,
    Ammo,
    Damage,
    Waiting,
    Cutting,
    Fusing,
    Building(BuildingKind),
    Color(GemColor),
    Error(BuildError),
}

impl Language {
    /// Deux-points typographique : espacé en français, collé en anglais.
    pub fn colon(self) -> &'static str {
        match self {
            Language::French => " : ",
            Language::English => ": ",
        }
    }

    pub fn tr(self, key: Tr) -> &'static str {
        use Language::*;
        match (key, self) {
            (Tr::Subtitle, French) => "Prototype M2 — tenir trois nuits",
            (Tr::Subtitle, English) => "Prototype M2 — hold for three nights",
            (Tr::Seed, French) => "Graine",
            (Tr::Seed, English) => "Seed",
            (Tr::Day, French) => "Jour",
            (Tr::Day, English) => "Day",
            (Tr::DayTime, French) => "jour",
            (Tr::DayTime, English) => "daytime",
            (Tr::NightTime, French) => "nuit",
            (Tr::NightTime, English) => "night",
            (Tr::Speed, French) => "Vitesse",
            (Tr::Speed, English) => "Speed",
            (Tr::Paused, French) => "PAUSE",
            (Tr::Paused, English) => "PAUSED",
            (Tr::Controls, French) => {
                "B : foreuse   C : convoyeur   G : taillerie   F : four   T : tour   X : démolir   R : tourner\n\
                 clic gauche : construire (glisser pour une ligne de convoyeurs)   Échap : annuler\n\
                 gemmes : clic sur une gemme de l'inventaire, puis sur une tour   U : retirer une gemme\n\
                 ZQSD / flèches : déplacer   A / E : pivoter   molette : zoom   clic droit + glisser : orienter\n\
                 H : retour au QG   Espace : pause   1 / 2 / 3 : vitesse   N : nouvelle planète   L : langue"
            }
            (Tr::Controls, English) => {
                "B: drill   C: conveyor   G: gem cutter   F: furnace   T: tower   X: demolish   R: rotate\n\
                 left click: build (drag for a line of conveyors)   Esc: cancel\n\
                 gems: click a gem in the inventory, then a tower   U: remove a gem\n\
                 WASD / arrows: move   Q / E: rotate   wheel: zoom   right click + drag: orbit\n\
                 H: back to HQ   Space: pause   1 / 2 / 3: speed   N: new planet   L: language"
            }
            (Tr::LanguageName, French) => "Langue : Français",
            (Tr::LanguageName, English) => "Language: English",
            (Tr::Crystals, French) => "Cristaux",
            (Tr::Crystals, English) => "Crystals",
            (Tr::HqHealth, French) => "QG",
            (Tr::HqHealth, English) => "HQ",
            (Tr::Wave, French) => "Vague",
            (Tr::Wave, English) => "Wave",
            (Tr::NightIn, French) => "La nuit tombe dans",
            (Tr::NightIn, English) => "Nightfall in",
            (Tr::NightNow, French) => "La Corruption attaque !",
            (Tr::NightNow, English) => "The Corruption attacks!",
            (Tr::Kills, French) => "ennemis détruits",
            (Tr::Kills, English) => "enemies destroyed",
            (Tr::Demolish, French) => "Démolir",
            (Tr::Demolish, English) => "Demolish",
            (Tr::Victory, French) => "VICTOIRE",
            (Tr::Victory, English) => "VICTORY",
            (Tr::VictoryDetail, French) => {
                "Trois nuits tenues : la Corruption recule.\nN : nouvelle planète"
            }
            (Tr::VictoryDetail, English) => {
                "Three nights held: the Corruption retreats.\nN: new planet"
            }
            (Tr::Defeat, French) => "DÉFAITE",
            (Tr::Defeat, English) => "DEFEAT",
            (Tr::DefeatDetail, French) => "Le QG est tombé.\nN : nouvelle planète",
            (Tr::DefeatDetail, English) => "The HQ has fallen.\nN: new planet",
            (Tr::Unsocket, French) => "Retirer",
            (Tr::Unsocket, English) => "Remove",
            (Tr::NeedsTower, French) => "Une gemme se sertit dans une tour",
            (Tr::NeedsTower, English) => "Gems go into towers",
            (Tr::Gems, French) => "Gemmes",
            (Tr::Gems, English) => "Gems",
            (Tr::NoGems, French) => {
                "Aucune gemme.\nFilon → taillerie → QG\n(3 cristaux d'une couleur = 1 gemme)"
            }
            (Tr::NoGems, English) => {
                "No gems yet.\nVein → gem cutter → HQ\n(3 crystals of one color = 1 gem)"
            }
            (Tr::Ammo, French) => "munitions",
            (Tr::Ammo, English) => "ammo",
            (Tr::Damage, French) => "dégâts",
            (Tr::Damage, English) => "damage",
            (Tr::Waiting, French) => "en attente",
            (Tr::Waiting, English) => "waiting",
            (Tr::Cutting, French) => "taille",
            (Tr::Cutting, English) => "cutting",
            (Tr::Fusing, French) => "fusion",
            (Tr::Fusing, English) => "fusing",
            (Tr::Building(kind), lang) => building_name(kind, lang),
            (Tr::Color(color), lang) => color_name(color, lang),
            (Tr::Error(error), lang) => error_text(error, lang),
        }
    }
}

fn building_name(kind: BuildingKind, lang: Language) -> &'static str {
    let french = lang == Language::French;
    match kind {
        BuildingKind::Hq => "QG",
        BuildingKind::Drill if french => "Foreuse",
        BuildingKind::Drill => "Drill",
        BuildingKind::Conveyor if french => "Convoyeur",
        BuildingKind::Conveyor => "Conveyor",
        BuildingKind::Cutter if french => "Taillerie",
        BuildingKind::Cutter => "Gem cutter",
        BuildingKind::Furnace if french => "Four",
        BuildingKind::Furnace => "Furnace",
        BuildingKind::Tower if french => "Tour",
        BuildingKind::Tower => "Tower",
    }
}

fn color_name(color: GemColor, lang: Language) -> &'static str {
    let french = lang == Language::French;
    match color {
        GemColor::Red if french => "Rouge",
        GemColor::Red => "Red",
        GemColor::Yellow if french => "Jaune",
        GemColor::Yellow => "Yellow",
        GemColor::Green if french => "Vert",
        GemColor::Green => "Green",
        GemColor::Cyan => "Cyan",
        GemColor::Purple if french => "Violet",
        GemColor::Purple => "Purple",
    }
}

impl Language {
    /// Nom d'une gemme : « Rouge-Jaune G3 ».
    pub fn gem_name(self, gem: Gem) -> String {
        let colors: Vec<&str> = gem.colors().map(|c| color_name(c, self)).collect();
        format!("{} G{}", colors.join("-"), gem.grade)
    }
}

fn error_text(error: BuildError, lang: Language) -> &'static str {
    let french = lang == Language::French;
    match error {
        BuildError::OutOfBounds if french => "Hors de la carte",
        BuildError::OutOfBounds => "Outside the map",
        BuildError::Occupied if french => "Case déjà occupée",
        BuildError::Occupied => "Tile already occupied",
        BuildError::Water if french => "Impossible de construire sur l'eau",
        BuildError::Water => "Cannot build on water",
        BuildError::NeedsCrystalVein if french => "Une foreuse se pose sur un filon de cristal",
        BuildError::NeedsCrystalVein => "A drill must be placed on a crystal vein",
        BuildError::NotEnoughCrystals if french => "Pas assez de cristaux",
        BuildError::NotEnoughCrystals => "Not enough crystals",
    }
}

pub struct I18nPlugin;

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Language>()
            .add_systems(Update, toggle_language);
    }
}

fn toggle_language(keys: Res<ButtonInput<KeyCode>>, mut language: ResMut<Language>) {
    if keys.just_pressed(KeyCode::KeyL) {
        *language = match *language {
            Language::French => Language::English,
            Language::English => Language::French,
        };
    }
}
