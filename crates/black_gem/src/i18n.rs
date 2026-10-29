//! Textes de l'interface en français et en anglais. Touche [L] pour changer de langue.

use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    French,
    English,
}

/// Identifiant d'un texte de l'interface.
#[derive(Clone, Copy, Debug)]
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
}

impl Language {
    pub fn tr(self, key: Tr) -> &'static str {
        use Language::*;
        match (key, self) {
            (Tr::Subtitle, French) => "Prototype M0 — atterrissage",
            (Tr::Subtitle, English) => "Prototype M0 — landing",
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
                "ZQSD / flèches : déplacer   A / E : pivoter   molette : zoom\n\
                 clic droit + glisser : orienter   H : retour au QG\n\
                 Espace : pause   1 / 2 / 3 : vitesse   N : nouvelle planète   L : langue"
            }
            (Tr::Controls, English) => {
                "WASD / arrows: move   Q / E: rotate   wheel: zoom\n\
                 right click + drag: orbit   H: back to HQ\n\
                 Space: pause   1 / 2 / 3: speed   N: new planet   L: language"
            }
            (Tr::LanguageName, French) => "Langue : Français",
            (Tr::LanguageName, English) => "Language: English",
        }
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
