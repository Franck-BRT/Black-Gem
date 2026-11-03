//! Le sous-ensemble Rust du jeu (Acte II).
//!
//! Le programme est analysé, puis **vérifié comme le ferait `rustc`** : types, mutabilité
//! (`let mut`), ownership (une valeur déplacée n'est plus utilisable) et emprunts (`&` / `&mut`).
//! Il est ensuite traduit dans l'arbre syntaxique commun et compilé pour la même machine
//! virtuelle que Python.
//!
//! Simplifications (documentées pour le joueur) : tous les entiers sont des `i64` ; une
//! référence `&mut` ne peut pas être gardée dans une variable (elle se passe en argument) ;
//! une fermeture rangée dans une variable garde une copie de ce qu'elle capture ; pas de
//! `dyn Trait`, de structures génériques ni de durées de vie explicites.

mod ast;
mod lexer;
mod lower;
mod parser;
mod types;

#[cfg(test)]
mod tests;

use crate::{compiler::Program, error::ScriptError, value::Value};

/// Type d'un paramètre ou d'un résultat de fonction du jeu.
#[derive(Clone, Debug, PartialEq)]
pub enum HostType {
    Int,
    Bool,
    Str,
    Unit,
    Tuple(Vec<HostType>),
    Vec(Box<HostType>),
    Option(Box<HostType>),
    Result(Box<HostType>, Box<HostType>),
    /// Type du jeu déclaré dans [`RustApi::enums`] ou [`RustApi::opaques`], ou alias.
    Named(&'static str),
}

/// Fonction du jeu, vue depuis Rust.
#[derive(Clone, Debug)]
pub struct RustFunction {
    pub names: Vec<&'static str>,
    /// Identifiant de la fonction pour [`crate::Host::call`].
    pub host: u32,
    pub params: Vec<HostType>,
    pub ret: HostType,
}

/// Énumération du jeu (comme `Direction::Nord`).
#[derive(Clone, Debug)]
pub struct RustEnum {
    pub names: Vec<&'static str>,
    pub variants: Vec<(Vec<&'static str>, Value)>,
}

/// Type du jeu dont les champs se lisent par des méthodes (comme `objet.grade()`). Il n'est
/// ni copiable ni clonable : c'est une ressource unique, qui se déplace.
#[derive(Clone, Debug)]
pub struct RustOpaque {
    pub names: Vec<&'static str>,
    pub fields: Vec<(Vec<&'static str>, HostType)>,
}

/// Ce que le jeu met à disposition des programmes Rust.
#[derive(Clone, Debug, Default)]
pub struct RustApi {
    pub functions: Vec<RustFunction>,
    pub enums: Vec<RustEnum>,
    pub opaques: Vec<RustOpaque>,
    /// Alias de types (comme `Position` = `(i64, i64)`).
    pub aliases: Vec<(Vec<&'static str>, HostType)>,
    /// Fonction du jeu qui affiche un texte (pour `println!`).
    pub print: u32,
}

/// Compile un programme Rust.
pub fn compile_rust(source: &str, api: &RustApi) -> Result<Program, ScriptError> {
    let items = parser::parse(source)?;
    lower::lower(&items, api)
}

/// Le programme est-il écrit en Rust ? (il contient une ligne `fn nom(`).
pub fn looks_like_rust(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim_start();
        let line = line.strip_prefix("pub ").unwrap_or(line);
        line.strip_prefix("fn ").is_some_and(|rest| {
            let rest = rest.trim_start();
            let name_len = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            name_len > 0 && rest[name_len..].trim_start().starts_with('(')
        })
    })
}
