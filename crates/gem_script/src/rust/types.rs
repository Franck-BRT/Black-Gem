//! Types du sous-ensemble Rust : inférence par unification, traits dérivés (`Copy`,
//! `Clone`, `Debug`, `PartialEq`) et description des types pour l'affichage.

use crate::value::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// Tous les entiers (`i32`, `u8`, `usize`…) sont des `i64` dans le jeu.
    Int,
    Bool,
    /// `&str` (et `char`).
    Str,
    String,
    Unit,
    /// Type des expressions qui ne terminent pas (`return`, `break`, `panic!`…).
    Never,
    Range,
    Tuple(Vec<Ty>),
    Vec(Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Ref(bool, Box<Ty>),
    /// Itérateur (`v.iter()`, `.enumerate()`…), représenté par la liste de ses éléments.
    Iter(Box<Ty>),
    /// Structure, énumération ou type du jeu (indice dans [`Types::adts`]).
    Adt(usize),
    /// Type encore inconnu, à déduire.
    Var(u32),
}

#[derive(Clone, Debug)]
pub enum AdtKind {
    Struct {
        fields: Vec<(String, Ty)>,
    },
    /// Énumération du joueur : valeur `(nom de la variante, données…)`.
    Enum {
        variants: Vec<(String, Vec<Ty>)>,
    },
    /// Énumération du jeu : chaque variante (noms français et anglais) a sa valeur.
    GameEnum {
        variants: Vec<(Vec<String>, Value)>,
    },
    /// Type du jeu aux champs privés, lus par des méthodes : valeur `(champ0, champ1…)`.
    Opaque {
        fields: Vec<(Vec<String>, Ty)>,
    },
}

#[derive(Clone, Debug)]
pub struct Adt {
    pub name: String,
    pub kind: AdtKind,
    pub copy: bool,
    pub clone: bool,
    pub debug: bool,
    pub eq: bool,
}

#[derive(Default)]
pub struct Types {
    pub adts: Vec<Adt>,
    vars: Vec<Option<Ty>>,
}

impl Types {
    pub fn fresh(&mut self) -> Ty {
        self.vars.push(None);
        Ty::Var(self.vars.len() as u32 - 1)
    }

    /// Remplace les variables connues (en surface).
    pub fn shallow(&self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        while let Ty::Var(v) = ty {
            match &self.vars[v as usize] {
                Some(known) => ty = known.clone(),
                None => break,
            }
        }
        ty
    }

    /// Le type sans ses références (`&&T` → `T`).
    pub fn strip(&self, ty: &Ty) -> Ty {
        match self.shallow(ty) {
            Ty::Ref(_, inner) => self.strip(&inner),
            other => other,
        }
    }

    fn occurs(&self, var: u32, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::Var(v) => v == var,
            Ty::Tuple(items) => items.iter().any(|t| self.occurs(var, t)),
            Ty::Vec(t) | Ty::Option(t) | Ty::Iter(t) | Ty::Ref(_, t) => self.occurs(var, &t),
            Ty::Result(t, e) => self.occurs(var, &t) || self.occurs(var, &e),
            _ => false,
        }
    }

    /// Rend les deux types égaux si possible.
    pub fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (&a, &b) {
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), other) | (other, Ty::Var(x)) => {
                if self.occurs(*x, other) {
                    return false;
                }
                self.vars[*x as usize] = Some(other.clone());
                true
            }
            (Ty::Never, _) | (_, Ty::Never) => true,
            (Ty::Tuple(x), Ty::Tuple(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(p, q)| self.unify(p, q))
            }
            (Ty::Vec(x), Ty::Vec(y))
            | (Ty::Option(x), Ty::Option(y))
            | (Ty::Iter(x), Ty::Iter(y)) => self.unify(x, y),
            (Ty::Result(x1, x2), Ty::Result(y1, y2)) => self.unify(x1, y1) && self.unify(x2, y2),
            (Ty::Ref(m1, x), Ty::Ref(m2, y)) => m1 == m2 && self.unify(x, y),
            _ => a == b,
        }
    }

    pub fn display(&self, ty: &Ty) -> String {
        match self.shallow(ty) {
            Ty::Int => "i64".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "&str".into(),
            Ty::String => "String".into(),
            Ty::Unit => "()".into(),
            Ty::Never => "!".into(),
            Ty::Range => "Range<i64>".into(),
            Ty::Tuple(items) => {
                let inner: Vec<String> = items.iter().map(|t| self.display(t)).collect();
                if items.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            Ty::Vec(t) => format!("Vec<{}>", self.display(&t)),
            Ty::Option(t) => format!("Option<{}>", self.display(&t)),
            Ty::Iter(t) => format!("Iterator<Item = {}>", self.display(&t)),
            Ty::Result(t, e) => format!("Result<{}, {}>", self.display(&t), self.display(&e)),
            Ty::Ref(true, t) => format!("&mut {}", self.display(&t)),
            Ty::Ref(false, t) => format!("&{}", self.display(&t)),
            Ty::Adt(i) => self.adts[i].name.clone(),
            Ty::Var(_) => "_".into(),
        }
    }

    /// `Copy` : la valeur est copiée au lieu d'être déplacée. Les types encore inconnus sont
    /// considérés comme copiables (on ne signale une erreur que si on en est sûr).
    pub fn is_copy(&self, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::Int | Ty::Bool | Ty::Str | Ty::Unit | Ty::Never | Ty::Range | Ty::Var(_) => true,
            // `&mut T` n'est pas Copy en Rust, mais il est « ré-emprunté » à chaque usage.
            Ty::Ref(..) => true,
            Ty::Tuple(items) => items.iter().all(|t| self.is_copy(t)),
            Ty::Option(t) => self.is_copy(&t),
            Ty::Result(t, e) => self.is_copy(&t) && self.is_copy(&e),
            Ty::String | Ty::Vec(_) | Ty::Iter(_) => false,
            Ty::Adt(i) => self.adts[i].copy,
        }
    }

    pub fn is_clone(&self, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::String => true,
            Ty::Vec(t) | Ty::Option(t) | Ty::Iter(t) => self.is_clone(&t),
            Ty::Ref(_, t) => self.is_clone(&t),
            Ty::Tuple(items) => items.iter().all(|t| self.is_clone(t)),
            Ty::Result(t, e) => self.is_clone(&t) && self.is_clone(&e),
            Ty::Adt(i) => self.adts[i].clone || self.adts[i].copy,
            other => self.is_copy(&other),
        }
    }

    pub fn has_debug(&self, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::Vec(t) | Ty::Option(t) | Ty::Ref(_, t) => self.has_debug(&t),
            Ty::Tuple(items) => items.iter().all(|t| self.has_debug(t)),
            Ty::Result(t, e) => self.has_debug(&t) && self.has_debug(&e),
            Ty::Adt(i) => self.adts[i].debug,
            Ty::Iter(_) => false,
            _ => true,
        }
    }

    pub fn has_display(&self, ty: &Ty) -> bool {
        matches!(
            self.strip(ty),
            Ty::Int | Ty::Bool | Ty::Str | Ty::String | Ty::Var(_) | Ty::Never
        )
    }

    pub fn has_eq(&self, ty: &Ty) -> bool {
        match self.shallow(ty) {
            Ty::Vec(t) | Ty::Option(t) | Ty::Ref(_, t) => self.has_eq(&t),
            Ty::Tuple(items) => items.iter().all(|t| self.has_eq(t)),
            Ty::Result(t, e) => self.has_eq(&t) && self.has_eq(&e),
            Ty::Adt(i) => self.adts[i].eq,
            Ty::Iter(_) | Ty::Range => false,
            _ => true,
        }
    }

    /// Description du type pour l'affichage (voir [`crate::format`]).
    pub fn descriptor(&self, ty: &Ty) -> Value {
        self.descriptor_depth(ty, 0)
    }

    fn descriptor_depth(&self, ty: &Ty, depth: u32) -> Value {
        let s = |text: &str| Value::Str(text.into());
        if depth > 8 {
            return s("?");
        }
        let d = |t: &Ty| self.descriptor_depth(t, depth + 1);
        match self.shallow(ty) {
            Ty::Int => s("i"),
            Ty::Bool => s("b"),
            Ty::Str | Ty::String => s("s"),
            Ty::Unit => s("u"),
            Ty::Range => s("range"),
            Ty::Never | Ty::Var(_) => s("?"),
            Ty::Ref(_, t) => d(&t),
            Ty::Tuple(items) => {
                let mut parts = vec![s("t")];
                parts.extend(items.iter().map(d));
                Value::Tuple(parts)
            }
            Ty::Vec(t) | Ty::Iter(t) => Value::Tuple(vec![s("v"), d(&t)]),
            Ty::Option(t) => Value::Tuple(vec![s("o"), d(&t)]),
            Ty::Result(t, e) => Value::Tuple(vec![s("res"), d(&t), d(&e)]),
            Ty::Adt(i) => {
                let adt = &self.adts[i];
                match &adt.kind {
                    AdtKind::Struct { fields } => {
                        let mut parts = vec![s("S"), s(&adt.name)];
                        for (name, field) in fields {
                            parts.push(Value::Tuple(vec![s(name), d(field)]));
                        }
                        Value::Tuple(parts)
                    }
                    AdtKind::Opaque { fields } => {
                        let mut parts = vec![s("S"), s(&adt.name)];
                        for (names, field) in fields {
                            parts.push(Value::Tuple(vec![s(&names[0]), d(field)]));
                        }
                        Value::Tuple(parts)
                    }
                    AdtKind::Enum { variants } => {
                        let mut parts = vec![s("E"), s(&adt.name)];
                        for (name, payload) in variants {
                            let mut variant = vec![s(name)];
                            variant.extend(payload.iter().map(d));
                            parts.push(Value::Tuple(variant));
                        }
                        Value::Tuple(parts)
                    }
                    AdtKind::GameEnum { variants } => {
                        let mut parts = vec![s("e")];
                        parts.extend(variants.iter().map(|(names, _)| s(&names[0])));
                        Value::Tuple(parts)
                    }
                }
            }
        }
    }
}
