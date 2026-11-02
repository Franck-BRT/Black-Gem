//! Vérification (types, mutabilité, ownership, emprunts) et traduction d'un programme Rust
//! vers l'arbre syntaxique commun.
//!
//! Représentation à l'exécution :
//! - `Some(x)` est `x` et `None` est `None` ; `Ok(x)` est `(true, x)`, `Err(e)` est `(false, e)` ;
//! - une structure est la liste de ses champs, une variante d'énumération est
//!   `("Variante", données…)` ;
//! - une référence est la valeur elle-même : `&mut x` est passé à la fonction, qui renvoie sa
//!   nouvelle valeur avec son résultat, et l'appelant la range dans `x` (« copie-retour ») ;
//! - `fn main` devient le programme principal (ses variables s'affichent dans l'inspecteur).

use std::{
    collections::{BTreeSet, HashMap},
    mem,
};

use super::{
    HostType, RustApi,
    ast::*,
    parser::unsupported,
    types::{Adt, AdtKind, Ty, Types},
};
use crate::{
    ast::{self as c, BinOp as CBin, PanicKind, StmtKind as K, Target},
    compiler::{self, Api, Builtin, Program},
    error::{ErrorKind, ScriptError, edit_distance},
    value::Value,
};

type R<T> = Result<T, ScriptError>;
type CExpr = c::Expr;
type CStmt = c::Stmt;

static WILD: Pat = Pat::Wild;

fn rerr(
    line: u32,
    code: &'static str,
    french: impl Into<String>,
    english: impl Into<String>,
) -> ScriptError {
    ScriptError::new(
        line,
        ErrorKind::Rust {
            code,
            french: french.into(),
            english: english.into(),
        },
    )
}

fn unit() -> CExpr {
    CExpr::Tuple(Vec::new())
}

fn int(n: i64) -> CExpr {
    CExpr::Int(n)
}

fn name(slot: &str) -> CExpr {
    CExpr::Name(slot.to_string())
}

fn index(object: CExpr, i: CExpr) -> CExpr {
    CExpr::Index(Box::new(object), Box::new(i))
}

fn bin(op: CBin, a: CExpr, b: CExpr) -> CExpr {
    CExpr::Binary(op, Box::new(a), Box::new(b))
}

fn not(e: CExpr) -> CExpr {
    CExpr::Not(Box::new(e))
}

fn and_all(tests: Vec<CExpr>) -> Option<CExpr> {
    tests
        .into_iter()
        .reduce(|a, b| CExpr::And(Box::new(a), Box::new(b)))
}

/// Une expression sans effet, qu'on peut évaluer plusieurs fois.
fn is_const(e: &CExpr) -> bool {
    match e {
        CExpr::Int(_) | CExpr::Str(_) | CExpr::Bool(_) | CExpr::None | CExpr::Const(_) => true,
        CExpr::Tuple(items) | CExpr::List(items) => items.iter().all(is_const),
        _ => false,
    }
}

fn is_simple(e: &CExpr) -> bool {
    is_const(e) || matches!(e, CExpr::Name(_))
}

fn is_upper(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

fn is_prefix(a: &[u32], b: &[u32]) -> bool {
    a.len() <= b.len() && a == &b[..a.len()]
}

#[derive(Clone, Debug)]
struct FnSig {
    slot: String,
    name: String,
    self_kind: Option<SelfKind>,
    self_ty: Option<Ty>,
    params: Vec<Ty>,
    ret: Ty,
    /// La fonction reçoit des `&mut` : elle renvoie `(résultat, nouvelles valeurs…)`.
    has_mut: bool,
}

#[derive(Clone, Debug)]
struct Local {
    name: String,
    slot: String,
    ty: Ty,
    mutable: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Moved {
    local: usize,
    path: Vec<u32>,
    line: u32,
    maybe: bool,
}

/// État de l'analyse sur un chemin d'exécution.
#[derive(Clone, Debug)]
struct Flow {
    reachable: bool,
    moved: Vec<Moved>,
    uninit: BTreeSet<usize>,
}

impl Flow {
    fn new() -> Self {
        Self {
            reachable: true,
            moved: Vec::new(),
            uninit: BTreeSet::new(),
        }
    }

    fn unreachable() -> Self {
        Self {
            reachable: false,
            ..Self::new()
        }
    }
}

fn merge(a: Flow, b: Flow) -> Flow {
    if !a.reachable {
        return b;
    }
    if !b.reachable {
        return a;
    }
    let mut moved = Vec::new();
    for m in &a.moved {
        let other = b
            .moved
            .iter()
            .find(|o| o.local == m.local && o.path == m.path);
        let mut m = m.clone();
        m.maybe = m.maybe || other.is_none_or(|o| o.maybe);
        moved.push(m);
    }
    for m in &b.moved {
        if !a
            .moved
            .iter()
            .any(|o| o.local == m.local && o.path == m.path)
        {
            let mut m = m.clone();
            m.maybe = true;
            moved.push(m);
        }
    }
    Flow {
        reachable: true,
        moved,
        uninit: a.uninit.union(&b.uninit).copied().collect(),
    }
}

struct LoopCtx {
    /// Les variables d'indice inférieur existaient avant la boucle.
    boundary: usize,
    breaks: Vec<Flow>,
    continues: Vec<Flow>,
    /// Pour chaque variable extérieure : sa première utilisation dans la boucle est-elle
    /// une lecture (`true`) ou une affectation (`false`) ?
    touched: HashMap<usize, bool>,
    is_loop: bool,
    /// `break valeur` (uniquement pour `loop`).
    value: Option<(String, Ty)>,
    /// À exécuter avant de quitter un tour de boucle (`for x in v.iter_mut()`).
    writeback: Vec<CStmt>,
}

struct FnState {
    ret: Ty,
    is_main: bool,
    locals: Vec<Local>,
    scopes: Vec<Vec<usize>>,
    flow: Flow,
    loops: Vec<LoopCtx>,
    /// Paramètres `&mut` : renvoyés avec le résultat.
    mut_params: Vec<String>,
}

impl FnState {
    fn new(ret: Ty, is_main: bool) -> Self {
        Self {
            ret,
            is_main,
            locals: Vec::new(),
            scopes: vec![Vec::new()],
            flow: Flow::new(),
            loops: Vec::new(),
            mut_params: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum Elem {
    Field { index: u32, tuple: Option<usize> },
    Index(CExpr),
}

/// Un emplacement : une variable, éventuellement suivie de champs et d'indices.
#[derive(Clone, Debug)]
struct Place {
    local: usize,
    elems: Vec<Elem>,
    ty: Ty,
    /// Mutabilité de la dernière référence traversée (aucune : la variable elle-même).
    through: Option<bool>,
    /// Chemin de champs suivi pour les déplacements (jusqu'au premier indice).
    move_path: Vec<u32>,
    /// Déplacer hors de cet emplacement est impossible (derrière une référence, dans un Vec).
    blocked: Option<Blocked>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blocked {
    Ref,
    Index,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Use {
    Shared,
    Mut,
    Move,
}

struct Binding {
    name: String,
    mutable: bool,
    ty: Ty,
    value: CExpr,
    /// La valeur est prise (et pas empruntée).
    by_value: bool,
    behind_ref: bool,
}

struct Lowerer<'a> {
    api: &'a RustApi,
    types: Types,
    type_names: HashMap<String, Ty>,
    fns: HashMap<String, FnSig>,
    consts: HashMap<String, Ty>,
    hosts: HashMap<&'static str, usize>,
    f: FnState,
    temps: u32,
    global_types: HashMap<String, Ty>,
    self_ty: Option<Ty>,
}

pub fn lower(items: &[Item], api: &RustApi) -> R<Program> {
    let mut l = Lowerer {
        api,
        types: Types::default(),
        type_names: HashMap::new(),
        fns: HashMap::new(),
        consts: HashMap::new(),
        hosts: HashMap::new(),
        f: FnState::new(Ty::Unit, true),
        temps: 0,
        global_types: HashMap::new(),
        self_ty: None,
    };
    for (i, function) in api.functions.iter().enumerate() {
        for name in &function.names {
            l.hosts.insert(name, i);
        }
    }
    l.declare_game_types();
    l.declare_types(items)?;
    l.declare_functions(items)?;

    let mut program = Vec::new();
    for item in items {
        if let Item::Const {
            name: const_name,
            line,
            ty,
            value,
        } = item
        {
            l.f = FnState::new(Ty::Unit, true);
            let declared = l.ty(ty, *line)?;
            let (v, t) = l.value(value, &mut program)?;
            l.expect(&declared, &t, *line)?;
            if l.consts
                .insert(const_name.clone(), declared.clone())
                .is_some()
            {
                return Err(rerr(
                    *line,
                    "E0428",
                    format!("« {const_name} » est défini deux fois"),
                    format!("'{const_name}' is defined twice"),
                ));
            }
            l.global_types.insert(const_name.clone(), declared);
            program.push(CStmt {
                line: *line,
                kind: K::Assign(Target::Name(const_name.clone()), v),
            });
        }
    }
    let mut main = None;
    for item in items {
        match item {
            Item::Fn(decl) if decl.name == "main" => main = Some(decl),
            Item::Fn(decl) => {
                let sig = l.fns[&decl.name].clone();
                l.self_ty = None;
                let def = l.function(decl, &sig)?;
                program.push(def);
            }
            Item::Impl {
                type_name, methods, ..
            } => {
                let ty = l.type_names[type_name].clone();
                let Ty::Adt(adt) = ty else { unreachable!() };
                for decl in methods {
                    let sig = l.fns[&format!("{adt}::{}", decl.name)].clone();
                    l.self_ty = Some(ty.clone());
                    let def = l.function(decl, &sig)?;
                    program.push(def);
                }
            }
            _ => {}
        }
    }
    l.self_ty = None;
    let Some(main) = main else {
        return Err(rerr(
            1,
            "E0601",
            "il manque la fonction principale : fn main() { … } (le golem commence par elle)",
            "the main function is missing: fn main() { … } (the golem starts there)",
        ));
    };
    if !main.params.is_empty() || main.self_param.is_some() || main.ret.is_some() {
        return Err(rerr(
            main.line,
            "E0580",
            "fn main() ne prend pas de paramètre et ne renvoie rien",
            "fn main() takes no parameter and returns nothing",
        ));
    }
    l.f = FnState::new(Ty::Unit, true);
    let mut body = Vec::new();
    let (_, ty) = l.block(&main.body, &mut body)?;
    l.expect(&Ty::Unit, &ty, main.body.end_line)?;
    program.extend(body);

    let mut compiled = compiler::compile(&program, &Api::default())?;
    compiled.global_types = compiled
        .globals
        .iter()
        .map(|g| l.global_types.get(g).map(|t| l.types.descriptor(t)))
        .collect();
    Ok(compiled)
}

impl Lowerer<'_> {
    // ----- Déclarations -----

    fn declare_game_types(&mut self) {
        for game_enum in &self.api.enums {
            let index = self.types.adts.len();
            self.types.adts.push(Adt {
                name: game_enum.names[0].to_string(),
                kind: AdtKind::GameEnum {
                    variants: game_enum
                        .variants
                        .iter()
                        .map(|(names, v)| {
                            (names.iter().map(|n| n.to_string()).collect(), v.clone())
                        })
                        .collect(),
                },
                copy: true,
                clone: true,
                debug: true,
                eq: true,
            });
            for name in &game_enum.names {
                self.type_names.insert(name.to_string(), Ty::Adt(index));
            }
        }
        let mut pending = Vec::new();
        for opaque in &self.api.opaques {
            let index = self.types.adts.len();
            self.types.adts.push(Adt {
                name: opaque.names[0].to_string(),
                kind: AdtKind::Opaque { fields: Vec::new() },
                copy: false,
                clone: false,
                debug: true,
                eq: false,
            });
            for name in &opaque.names {
                self.type_names.insert(name.to_string(), Ty::Adt(index));
            }
            pending.push((index, opaque));
        }
        for (names, ty) in &self.api.aliases {
            let ty = self.host_ty(ty);
            for name in names {
                self.type_names.insert(name.to_string(), ty.clone());
            }
        }
        for (index, opaque) in pending {
            let fields = opaque
                .fields
                .iter()
                .map(|(names, ty)| {
                    (
                        names.iter().map(|n| n.to_string()).collect(),
                        self.host_ty(ty),
                    )
                })
                .collect();
            self.types.adts[index].kind = AdtKind::Opaque { fields };
        }
    }

    fn host_ty(&self, ty: &HostType) -> Ty {
        match ty {
            HostType::Int => Ty::Int,
            HostType::Bool => Ty::Bool,
            HostType::Str => Ty::Str,
            HostType::Unit => Ty::Unit,
            HostType::Tuple(items) => Ty::Tuple(items.iter().map(|t| self.host_ty(t)).collect()),
            HostType::Vec(t) => Ty::Vec(Box::new(self.host_ty(t))),
            HostType::Option(t) => Ty::Option(Box::new(self.host_ty(t))),
            HostType::Result(t, e) => {
                Ty::Result(Box::new(self.host_ty(t)), Box::new(self.host_ty(e)))
            }
            HostType::Named(n) => self.type_names.get(*n).cloned().unwrap_or(Ty::Unit),
        }
    }

    fn declare_types(&mut self, items: &[Item]) -> R<()> {
        let mut declared = Vec::new();
        for item in items {
            let (type_name, line, derives) = match item {
                Item::Struct(s) => (&s.name, s.line, &s.derives),
                Item::Enum(e) => (&e.name, e.line, &e.derives),
                _ => continue,
            };
            if self.type_names.contains_key(type_name) {
                return Err(rerr(
                    line,
                    "E0428",
                    format!(
                        "le type « {type_name} » est défini deux fois (ou existe déjà dans le jeu)"
                    ),
                    format!(
                        "the type '{type_name}' is defined twice (or already exists in the game)"
                    ),
                ));
            }
            let has = |d: &str| derives.iter().any(|x| x == d);
            let index = self.types.adts.len();
            self.types.adts.push(Adt {
                name: type_name.clone(),
                kind: AdtKind::Struct { fields: Vec::new() },
                copy: has("Copy"),
                clone: has("Clone"),
                debug: has("Debug"),
                eq: has("PartialEq"),
            });
            if has("Copy") && !has("Clone") {
                return Err(rerr(
                    line,
                    "E0277",
                    "Copy demande aussi Clone : écris #[derive(Clone, Copy)]",
                    "Copy also requires Clone: write #[derive(Clone, Copy)]",
                ));
            }
            self.type_names.insert(type_name.clone(), Ty::Adt(index));
            declared.push((index, item));
        }
        for (index, item) in declared {
            self.self_ty = Some(Ty::Adt(index));
            let (kind, line) = match item {
                Item::Struct(s) => {
                    let mut fields: Vec<(String, Ty)> = Vec::new();
                    for (field, ty) in &s.fields {
                        if fields.iter().any(|(f, _)| f == field) {
                            return Err(rerr(
                                s.line,
                                "E0124",
                                format!("le champ « {field} » est déclaré deux fois"),
                                format!("the field '{field}' is declared twice"),
                            ));
                        }
                        let ty = self.ty(ty, s.line)?;
                        self.no_ref_inside(&ty, s.line)?;
                        fields.push((field.clone(), ty));
                    }
                    (AdtKind::Struct { fields }, s.line)
                }
                Item::Enum(e) => {
                    let mut variants: Vec<(String, Vec<Ty>)> = Vec::new();
                    for (variant, payload) in &e.variants {
                        let tys = payload
                            .iter()
                            .map(|t| self.ty(t, e.line))
                            .collect::<R<Vec<_>>>()?;
                        for ty in &tys {
                            self.no_ref_inside(ty, e.line)?;
                        }
                        variants.push((variant.clone(), tys));
                    }
                    (AdtKind::Enum { variants }, e.line)
                }
                _ => unreachable!(),
            };
            self.types.adts[index].kind = kind;
            let _ = line;
        }
        self.self_ty = None;
        // Copy / Clone : tous les champs doivent l'être aussi.
        for index in 0..self.types.adts.len() {
            let adt = self.types.adts[index].clone();
            let tys: Vec<Ty> = match &adt.kind {
                AdtKind::Struct { fields } => fields.iter().map(|(_, t)| t.clone()).collect(),
                AdtKind::Enum { variants } => {
                    variants.iter().flat_map(|(_, p)| p.clone()).collect()
                }
                _ => continue,
            };
            let line = items
                .iter()
                .find_map(|i| match i {
                    Item::Struct(s) if s.name == adt.name => Some(s.line),
                    Item::Enum(e) if e.name == adt.name => Some(e.line),
                    _ => None,
                })
                .unwrap_or(1);
            for ty in &tys {
                if adt.copy && !self.types.is_copy(ty) {
                    return Err(rerr(
                        line,
                        "E0204",
                        format!(
                            "« {} » ne peut pas être Copy : son champ de type « {} » ne l'est pas",
                            adt.name,
                            self.types.display(ty)
                        ),
                        format!(
                            "'{}' cannot be Copy: its field of type '{}' is not",
                            adt.name,
                            self.types.display(ty)
                        ),
                    ));
                }
                if adt.clone && !self.types.is_clone(ty) {
                    return Err(rerr(
                        line,
                        "E0277",
                        format!(
                            "« {} » ne peut pas être Clone : son champ de type « {} » ne l'est pas",
                            adt.name,
                            self.types.display(ty)
                        ),
                        format!(
                            "'{}' cannot be Clone: its field of type '{}' is not",
                            adt.name,
                            self.types.display(ty)
                        ),
                    ));
                }
                if adt.debug && !self.types.has_debug(ty) {
                    return Err(rerr(
                        line,
                        "E0277",
                        format!(
                            "« {} » ne peut pas être Debug : ajoute #[derive(Debug)] au type « {} »",
                            adt.name,
                            self.types.display(ty)
                        ),
                        format!(
                            "'{}' cannot be Debug: add #[derive(Debug)] to the type '{}'",
                            adt.name,
                            self.types.display(ty)
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn no_ref_inside(&self, ty: &Ty, line: u32) -> R<()> {
        if matches!(self.types.shallow(ty), Ty::Ref(..)) {
            return Err(unsupported(
                line,
                "les références dans une structure (elles demandent des durées de vie)",
                "references inside a struct (they need lifetimes)",
            ));
        }
        Ok(())
    }

    fn signature(&mut self, decl: &FnDecl, slot: String, display: String) -> R<FnSig> {
        let mut params = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for p in &decl.params {
            if seen.contains(&p.name.as_str()) {
                return Err(rerr(
                    decl.line,
                    "E0415",
                    format!("le paramètre « {} » apparaît deux fois", p.name),
                    format!("the parameter '{}' appears twice", p.name),
                ));
            }
            seen.push(&p.name);
            params.push(self.ty(&p.ty, decl.line)?);
        }
        let ret = match &decl.ret {
            Some(t) => self.ty(t, decl.line)?,
            None => Ty::Unit,
        };
        if matches!(ret, Ty::Ref(true, _)) {
            return Err(unsupported(
                decl.line,
                "renvoyer une référence &mut",
                "returning a &mut reference",
            ));
        }
        let (self_kind, self_ty) = match (&decl.self_param, &self.self_ty) {
            (Some((kind, _)), Some(t)) => {
                let ty = match kind {
                    SelfKind::Value => t.clone(),
                    SelfKind::Ref => Ty::Ref(false, Box::new(t.clone())),
                    SelfKind::RefMut => Ty::Ref(true, Box::new(t.clone())),
                };
                (Some(*kind), Some(ty))
            }
            _ => (None, None),
        };
        let has_mut = self_kind == Some(SelfKind::RefMut)
            || params.iter().any(|t| matches!(t, Ty::Ref(true, _)));
        Ok(FnSig {
            slot,
            name: display,
            self_kind,
            self_ty,
            params,
            ret,
            has_mut,
        })
    }

    fn declare_functions(&mut self, items: &[Item]) -> R<()> {
        for item in items {
            match item {
                Item::Fn(decl) => {
                    self.self_ty = None;
                    let sig =
                        self.signature(decl, format!("fn {}", decl.name), decl.name.clone())?;
                    if self.fns.insert(decl.name.clone(), sig).is_some() {
                        return Err(rerr(
                            decl.line,
                            "E0428",
                            format!("la fonction « {} » est définie deux fois", decl.name),
                            format!("the function '{}' is defined twice", decl.name),
                        ));
                    }
                }
                Item::Impl {
                    type_name,
                    line,
                    methods,
                } => {
                    let Some(ty) = self.type_names.get(type_name).cloned() else {
                        return Err(self.unknown_type(type_name, *line));
                    };
                    let Ty::Adt(adt) = ty else {
                        return Err(rerr(
                            *line,
                            "E0116",
                            format!(
                                "impossible d'écrire impl pour « {type_name} » : seulement pour tes propres struct et enum"
                            ),
                            format!(
                                "cannot write impl for '{type_name}': only for your own structs and enums"
                            ),
                        ));
                    };
                    if matches!(
                        self.types.adts[adt].kind,
                        AdtKind::GameEnum { .. } | AdtKind::Opaque { .. }
                    ) {
                        return Err(rerr(
                            *line,
                            "E0116",
                            format!(
                                "impossible d'écrire impl pour « {type_name} », un type du jeu"
                            ),
                            format!("cannot write impl for '{type_name}', a game type"),
                        ));
                    }
                    self.self_ty = Some(ty.clone());
                    for decl in methods {
                        let key = format!("{adt}::{}", decl.name);
                        let sig = self.signature(
                            decl,
                            format!("{type_name}::{}", decl.name),
                            format!("{type_name}::{}", decl.name),
                        )?;
                        if self.fns.insert(key, sig).is_some() {
                            return Err(rerr(
                                decl.line,
                                "E0201",
                                format!("la méthode « {} » est définie deux fois", decl.name),
                                format!("the method '{}' is defined twice", decl.name),
                            ));
                        }
                    }
                    self.self_ty = None;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn unknown_type(&self, type_name: &str, line: u32) -> ScriptError {
        let mut candidates: Vec<&str> = self.type_names.keys().map(String::as_str).collect();
        candidates.extend([
            "i64", "i32", "u32", "usize", "bool", "String", "Vec", "Option", "Result",
        ]);
        let suggestion = suggest(type_name, candidates);
        rerr(
            line,
            "E0412",
            format!(
                "le type « {type_name} » est inconnu{}",
                hint_fr(&suggestion)
            ),
            format!("the type '{type_name}' is unknown{}", hint_en(&suggestion)),
        )
    }

    fn ty(&mut self, t: &TypeExpr, line: u32) -> R<Ty> {
        Ok(match t {
            TypeExpr::Path(type_name, args) => {
                let arg = |l: &mut Self, i: usize| -> R<Ty> {
                    match args.get(i) {
                        Some(t) => l.ty(t, line),
                        None => Ok(l.types.fresh()),
                    }
                };
                match type_name.as_str() {
                    "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32"
                    | "u64" | "u128" | "usize" => Ty::Int,
                    "bool" => Ty::Bool,
                    "str" | "char" => Ty::Str,
                    "String" => Ty::String,
                    "f32" | "f64" => return Err(ScriptError::new(line, ErrorKind::DecimalNumber)),
                    "Vec" | "VecDeque" => Ty::Vec(Box::new(arg(self, 0)?)),
                    "Option" => Ty::Option(Box::new(arg(self, 0)?)),
                    "Result" => Ty::Result(Box::new(arg(self, 0)?), Box::new(arg(self, 1)?)),
                    "!" => Ty::Never,
                    "Self" => match &self.self_ty {
                        Some(t) => t.clone(),
                        None => {
                            return Err(rerr(
                                line,
                                "E0411",
                                "« Self » n'existe que dans un bloc impl",
                                "'Self' only exists inside an impl block",
                            ));
                        }
                    },
                    "HashMap" | "HashSet" | "BTreeMap" | "Box" | "Rc" | "RefCell" => {
                        return Err(unsupported(
                            line,
                            &format!("le type {type_name}"),
                            &format!("the {type_name} type"),
                        ));
                    }
                    other => match self.type_names.get(other) {
                        Some(t) => t.clone(),
                        None => return Err(self.unknown_type(other, line)),
                    },
                }
            }
            TypeExpr::Ref(mutable, inner) => match &**inner {
                TypeExpr::Path(n, _) if n == "str" => Ty::Str,
                other => Ty::Ref(*mutable, Box::new(self.ty(other, line)?)),
            },
            TypeExpr::Tuple(items) if items.is_empty() => Ty::Unit,
            TypeExpr::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|t| self.ty(t, line))
                    .collect::<R<Vec<_>>>()?,
            ),
            TypeExpr::Array(inner) => Ty::Vec(Box::new(self.ty(inner, line)?)),
            TypeExpr::Infer => self.types.fresh(),
        })
    }

    // ----- Fonctions -----

    fn function(&mut self, decl: &FnDecl, sig: &FnSig) -> R<CStmt> {
        self.f = FnState::new(sig.ret.clone(), false);
        let mut params = Vec::new();
        if let (Some(kind), Some(ty)) = (sig.self_kind, &sig.self_ty) {
            let mutable = decl.self_param.is_some_and(|(_, m)| m);
            let slot = self.declare("self", ty.clone(), mutable);
            if kind == SelfKind::RefMut {
                self.f.mut_params.push(slot.clone());
            }
            params.push(slot);
        }
        for (p, ty) in decl.params.iter().zip(&sig.params) {
            let slot = self.declare(&p.name, ty.clone(), p.mutable);
            if matches!(ty, Ty::Ref(true, _)) {
                self.f.mut_params.push(slot.clone());
            }
            params.push(slot);
        }
        let mut body = Vec::new();
        let (value, ty) = self.block(&decl.body, &mut body)?;
        let line = decl
            .body
            .tail
            .as_ref()
            .map_or(decl.body.end_line, |t| t.line);
        if !self.types.unify(&sig.ret, &ty) {
            let missing_value =
                matches!(self.types.shallow(&ty), Ty::Unit) && decl.body.tail.is_none();
            let (fr_hint, en_hint) = if missing_value {
                (
                    " (un « ; » en trop après la dernière expression ? ou un return oublié ?)",
                    " (an extra ';' after the last expression? or a forgotten return?)",
                )
            } else {
                ("", "")
            };
            return Err(rerr(
                line,
                "E0308",
                format!(
                    "« {} » doit renvoyer « {} », mais son corps vaut « {} »{fr_hint}",
                    sig.name,
                    self.types.display(&sig.ret),
                    self.types.display(&ty)
                ),
                format!(
                    "'{}' must return '{}', but its body is '{}'{en_hint}",
                    sig.name,
                    self.types.display(&sig.ret),
                    self.types.display(&ty)
                ),
            ));
        }
        if self.f.flow.reachable {
            body.push(CStmt {
                line: decl.body.end_line,
                kind: K::Return(Some(self.return_value(value))),
            });
        }
        Ok(CStmt {
            line: decl.line,
            kind: K::Def(sig.slot.clone(), params, body),
        })
    }

    /// Valeur renvoyée (avec les nouvelles valeurs des paramètres `&mut`).
    fn return_value(&self, value: CExpr) -> CExpr {
        if self.f.mut_params.is_empty() {
            return value;
        }
        let mut items = vec![value];
        items.extend(self.f.mut_params.iter().map(|p| name(p)));
        CExpr::Tuple(items)
    }

    fn return_stmts(&self, value: CExpr, line: u32, out: &mut Vec<CStmt>) {
        for l in self.f.loops.iter().rev() {
            out.extend(l.writeback.iter().cloned());
        }
        if self.f.is_main {
            out.push(CStmt {
                line,
                kind: K::Exit,
            });
        } else {
            out.push(CStmt {
                line,
                kind: K::Return(Some(self.return_value(value))),
            });
        }
    }

    // ----- Variables -----

    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("__t{}", self.temps)
    }

    fn push_scope(&mut self) {
        self.f.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.f.scopes.pop();
    }

    fn declare(&mut self, var: &str, ty: Ty, mutable: bool) -> String {
        let scope = self.f.scopes.last().unwrap();
        let same_scope = scope
            .iter()
            .rev()
            .find(|&&id| self.f.locals[id].name == var)
            .map(|&id| self.f.locals[id].slot.clone());
        let slot = same_scope.unwrap_or_else(|| {
            let live: BTreeSet<&str> = self
                .f
                .scopes
                .iter()
                .flatten()
                .map(|&id| self.f.locals[id].slot.as_str())
                .collect();
            let mut slot = var.to_string();
            let mut n = 1;
            while live.contains(slot.as_str())
                || (self.f.is_main && self.consts.contains_key(&slot))
            {
                n += 1;
                slot = format!("{var}#{n}");
            }
            slot
        });
        self.f.locals.push(Local {
            name: var.to_string(),
            slot: slot.clone(),
            ty: ty.clone(),
            mutable,
        });
        let id = self.f.locals.len() - 1;
        self.f.scopes.last_mut().unwrap().push(id);
        if self.f.is_main {
            self.global_types.insert(slot.clone(), ty);
        }
        slot
    }

    fn lookup(&self, var: &str) -> Option<usize> {
        self.f
            .scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .copied()
            .find(|&id| self.f.locals[id].name == var)
    }

    fn unknown_name(&self, var: &str, line: u32) -> ScriptError {
        let mut candidates: Vec<&str> = self
            .f
            .scopes
            .iter()
            .flatten()
            .map(|&id| self.f.locals[id].name.as_str())
            .collect();
        candidates.extend(self.consts.keys().map(String::as_str));
        candidates.extend(
            self.fns
                .keys()
                .filter(|k| !k.contains("::"))
                .map(String::as_str),
        );
        candidates.extend(self.hosts.keys().copied());
        candidates.extend(["Some", "None", "Ok", "Err", "self"]);
        let suggestion = suggest(var, candidates);
        rerr(
            line,
            "E0425",
            format!("le nom « {var} » est inconnu{}", hint_fr(&suggestion)),
            format!("the name '{var}' is unknown{}", hint_en(&suggestion)),
        )
    }

    fn touch(&mut self, local: usize, is_use: bool) {
        for l in &mut self.f.loops {
            if local < l.boundary {
                l.touched.entry(local).or_insert(is_use);
            }
        }
    }

    fn check_use(&mut self, place: &Place, line: u32) -> R<()> {
        self.touch(place.local, true);
        if !self.f.flow.reachable {
            return Ok(());
        }
        let var = self.f.locals[place.local].name.clone();
        if self.f.flow.uninit.contains(&place.local) {
            return Err(rerr(
                line,
                "E0381",
                format!(
                    "« {var} » est utilisée avant d'avoir reçu une valeur (sur au moins un chemin)"
                ),
                format!("'{var}' is used before being given a value (on at least one path)"),
            ));
        }
        if let Some(m) = self
            .f
            .flow
            .moved
            .iter()
            .find(|m| {
                m.local == place.local
                    && (is_prefix(&m.path, &place.move_path)
                        || is_prefix(&place.move_path, &m.path))
            })
            .cloned()
        {
            let ty = self.f.locals[place.local].ty.clone();
            let game = matches!(
                self.types.strip(&ty),
                Ty::Adt(i) if matches!(self.types.adts[i].kind, AdtKind::Opaque { .. })
            ) || matches!(self.types.strip(&ty), Ty::Option(t) if matches!(self.types.strip(&t), Ty::Adt(i) if matches!(self.types.adts[i].kind, AdtKind::Opaque { .. })));
            let (fr_extra, en_extra) = if game {
                (
                    " Un objet du jeu est unique : une fois donné, le golem ne le possède plus.",
                    " A game item is unique: once given away, the golem no longer owns it.",
                )
            } else {
                (
                    " Prête-la avec & au lieu de la donner, ou fais une copie avec .clone().",
                    " Lend it with & instead of giving it away, or make a copy with .clone().",
                )
            };
            let (fr_when, en_when) = if m.maybe {
                ("a peut-être été déplacée", "may have been moved")
            } else {
                ("a été déplacée", "was moved")
            };
            return Err(rerr(
                line,
                "E0382",
                format!(
                    "la valeur de « {var} » {fr_when} ligne {} : elle n'est plus utilisable ici.{fr_extra}",
                    m.line
                ),
                format!(
                    "the value of '{var}' {en_when} on line {}: it can no longer be used here.{en_extra}",
                    m.line
                ),
            ));
        }
        Ok(())
    }

    fn do_move(&mut self, place: &Place, line: u32) -> R<()> {
        if self.types.is_copy(&place.ty) || !self.f.flow.reachable {
            return Ok(());
        }
        let shown = self.types.display(&place.ty);
        match place.blocked {
            Some(Blocked::Ref) => {
                return Err(rerr(
                    line,
                    "E0507",
                    format!(
                        "impossible de prendre cette valeur (« {shown} ») : elle est seulement empruntée (&). Utilise .clone() ou garde une référence"
                    ),
                    format!(
                        "cannot take this value ('{shown}'): it is only borrowed (&). Use .clone() or keep a reference"
                    ),
                ));
            }
            Some(Blocked::Index) => {
                return Err(rerr(
                    line,
                    "E0507",
                    format!(
                        "impossible de sortir une valeur (« {shown} ») d'un Vec avec [ ] : utilise .remove(i), .pop(), &v[i] ou .clone()"
                    ),
                    format!(
                        "cannot move a value ('{shown}') out of a Vec with [ ]: use .remove(i), .pop(), &v[i] or .clone()"
                    ),
                ));
            }
            None => {}
        }
        self.f.flow.moved.push(Moved {
            local: place.local,
            path: place.move_path.clone(),
            line,
            maybe: false,
        });
        Ok(())
    }

    /// Après une affectation : l'emplacement a de nouveau une valeur.
    fn reinit(&mut self, place: &Place) {
        if place.elems.is_empty() {
            self.touch(place.local, false);
            self.f.flow.uninit.remove(&place.local);
        }
        self.f
            .flow
            .moved
            .retain(|m| !(m.local == place.local && is_prefix(&place.move_path, &m.path)));
    }

    fn check_mutable(&self, place: &Place, line: u32, borrow: bool) -> R<()> {
        let local = &self.f.locals[place.local];
        match place.through {
            Some(true) => return Ok(()),
            Some(false) => {
                return Err(rerr(
                    line,
                    if borrow { "E0596" } else { "E0594" },
                    format!(
                        "impossible de modifier « {} » : on y accède par une référence partagée (&). Il faudrait une référence &mut",
                        local.name
                    ),
                    format!(
                        "cannot modify '{}': it is reached through a shared reference (&). A &mut reference is needed",
                        local.name
                    ),
                ));
            }
            None => {}
        }
        if local.mutable {
            return Ok(());
        }
        if !borrow && place.elems.is_empty() && self.f.flow.uninit.contains(&place.local) {
            return Ok(());
        }
        let (code, fr, en) = if borrow {
            (
                "E0596",
                format!(
                    "impossible d'emprunter « {0} » en écriture (&mut) : déclare-la avec let mut {0}",
                    local.name
                ),
                format!(
                    "cannot borrow '{0}' as mutable (&mut): declare it with let mut {0}",
                    local.name
                ),
            )
        } else if place.elems.is_empty() {
            (
                "E0384",
                format!(
                    "« {0} » n'est pas modifiable : déclare-la avec let mut {0}",
                    local.name
                ),
                format!(
                    "'{0}' cannot be changed: declare it with let mut {0}",
                    local.name
                ),
            )
        } else {
            (
                "E0594",
                format!(
                    "impossible de modifier une partie de « {0} » : déclare-la avec let mut {0}",
                    local.name
                ),
                format!(
                    "cannot modify part of '{0}': declare it with let mut {0}",
                    local.name
                ),
            )
        };
        Err(rerr(line, code, fr, en))
    }

    // ----- Emplacements -----

    fn is_place_expr(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Path(p) => p.len() == 1 && self.lookup(&p[0]).is_some(),
            ExprKind::Field(base, _) | ExprKind::Index(base, _) => self.is_place_expr(base),
            ExprKind::Unary(UnOp::Deref, base) => self.is_place_expr(base),
            _ => false,
        }
    }

    /// Passe à travers les références du type de l'emplacement (accès automatique).
    fn auto_deref(&self, place: &mut Place) {
        while let Ty::Ref(m, inner) = self.types.shallow(&place.ty) {
            place.through = Some(place.through.unwrap_or(true) && m);
            place.blocked = Some(Blocked::Ref);
            place.ty = *inner;
        }
    }

    fn place(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<Option<Place>> {
        if !self.is_place_expr(e) {
            return Ok(None);
        }
        Ok(Some(self.place_inner(e, out)?))
    }

    fn place_inner(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<Place> {
        match &e.kind {
            ExprKind::Path(p) => {
                let local = self.lookup(&p[0]).expect("variable");
                Ok(Place {
                    local,
                    elems: Vec::new(),
                    ty: self.f.locals[local].ty.clone(),
                    through: None,
                    move_path: Vec::new(),
                    blocked: None,
                })
            }
            ExprKind::Field(base, field) => {
                let mut place = self.place_inner(base, out)?;
                self.auto_deref(&mut place);
                let (index, ty, tuple) = self.field(&place.ty, field, e.line)?;
                if place
                    .elems
                    .iter()
                    .all(|el| matches!(el, Elem::Field { .. }))
                {
                    place.move_path.push(index);
                }
                place.elems.push(Elem::Field { index, tuple });
                place.ty = ty;
                Ok(place)
            }
            ExprKind::Index(base, idx) => {
                let mut place = self.place_inner(base, out)?;
                self.auto_deref(&mut place);
                let elem = self.index_elem_ty(&place.ty, e.line)?;
                let (i, ity) = self.value(idx, out)?;
                self.expect_int(&ity, idx.line, "[ ]")?;
                let i = if is_simple(&i) {
                    i
                } else {
                    let t = self.temp();
                    out.push(CStmt {
                        line: e.line,
                        kind: K::Assign(Target::Name(t.clone()), i),
                    });
                    name(&t)
                };
                place.elems.push(Elem::Index(i));
                place.blocked = Some(Blocked::Index);
                place.ty = elem;
                Ok(place)
            }
            ExprKind::Unary(UnOp::Deref, base) => {
                let mut place = self.place_inner(base, out)?;
                match self.types.shallow(&place.ty) {
                    Ty::Ref(m, inner) => {
                        place.through = Some(place.through.unwrap_or(true) && m);
                        place.blocked = Some(Blocked::Ref);
                        place.ty = *inner;
                        Ok(place)
                    }
                    other => Err(rerr(
                        e.line,
                        "E0614",
                        format!(
                            "« * » s'utilise sur une référence, pas sur « {} »",
                            self.types.display(&other)
                        ),
                        format!(
                            "'*' is used on a reference, not on '{}'",
                            self.types.display(&other)
                        ),
                    )),
                }
            }
            _ => unreachable!(),
        }
    }

    fn index_elem_ty(&mut self, ty: &Ty, line: u32) -> R<Ty> {
        match self.types.strip(ty) {
            Ty::Vec(t) => Ok(*t),
            Ty::Var(_) => {
                let elem = self.types.fresh();
                let resolved = self.types.strip(ty);
                self.types
                    .unify(&resolved, &Ty::Vec(Box::new(elem.clone())));
                Ok(elem)
            }
            Ty::Str | Ty::String => Err(rerr(
                line,
                "E0277",
                "on ne peut pas prendre un caractère d'un texte avec [ ] en Rust : utilise .chars()",
                "a text cannot be indexed with [ ] in Rust: use .chars()",
            )),
            other => Err(rerr(
                line,
                "E0608",
                format!(
                    "on ne peut pas utiliser [ ] sur « {} »",
                    self.types.display(&other)
                ),
                format!("cannot use [ ] on '{}'", self.types.display(&other)),
            )),
        }
    }

    /// Champ d'une structure ou d'un tuple : indice, type, et taille du tuple le cas échéant.
    fn field(&mut self, ty: &Ty, field: &str, line: u32) -> R<(u32, Ty, Option<usize>)> {
        let ty = self.types.strip(ty);
        match &ty {
            Ty::Tuple(items) => {
                if let Ok(i) = field.parse::<usize>()
                    && i < items.len()
                {
                    return Ok((i as u32, items[i].clone(), Some(items.len())));
                }
            }
            Ty::Adt(i) => match &self.types.adts[*i].kind {
                AdtKind::Struct { fields } => {
                    if let Some(pos) = fields.iter().position(|(f, _)| f == field) {
                        return Ok((pos as u32, fields[pos].1.clone(), None));
                    }
                }
                AdtKind::Opaque { fields }
                    if fields
                        .iter()
                        .any(|(names, _)| names.iter().any(|n| n == field)) =>
                {
                    return Err(rerr(
                        line,
                        "E0616",
                        format!("le champ « {field} » est privé : appelle la méthode .{field}()"),
                        format!("the field '{field}' is private: call the method .{field}()"),
                    ));
                }
                _ => {}
            },
            Ty::Var(_) => {
                return Err(rerr(
                    line,
                    "E0282",
                    "le type de cette valeur est encore inconnu : précise-le (let x: Type = …)",
                    "the type of this value is not known yet: specify it (let x: Type = …)",
                ));
            }
            _ => {}
        }
        Err(rerr(
            line,
            "E0609",
            format!(
                "le type « {} » n'a pas de champ « {field} »",
                self.types.display(&ty)
            ),
            format!(
                "the type '{}' has no field '{field}'",
                self.types.display(&ty)
            ),
        ))
    }

    fn place_expr(&self, place: &Place) -> CExpr {
        let mut e = name(&self.f.locals[place.local].slot);
        for elem in &place.elems {
            e = match elem {
                Elem::Field { index: i, .. } => index(e, int(*i as i64)),
                Elem::Index(i) => index(e, i.clone()),
            };
        }
        e
    }

    /// Range une valeur dans un emplacement (en reconstruisant les tuples traversés).
    fn store(&mut self, place: &Place, value: CExpr, line: u32, out: &mut Vec<CStmt>) {
        let slot = self.f.locals[place.local].slot.clone();
        let stmt = |kind| CStmt { line, kind };
        if place.elems.is_empty() {
            out.push(stmt(K::Assign(Target::Name(slot), value)));
            return;
        }
        // Lectures des conteneurs successifs : r0 = slot, r1 = r0[e1]…
        let mut reads = vec![name(&slot)];
        for elem in &place.elems[..place.elems.len() - 1] {
            let previous = reads.last().unwrap().clone();
            reads.push(match elem {
                Elem::Field { index: i, .. } => index(previous, int(*i as i64)),
                Elem::Index(i) => index(previous, i.clone()),
            });
        }
        let mut new_value = value;
        for k in (0..place.elems.len()).rev() {
            let container = reads[k].clone();
            match &place.elems[k] {
                Elem::Field {
                    index: i,
                    tuple: Some(arity),
                } => {
                    let items = (0..*arity)
                        .map(|j| {
                            if j == *i as usize {
                                new_value.clone()
                            } else {
                                index(container.clone(), int(j as i64))
                            }
                        })
                        .collect();
                    new_value = CExpr::Tuple(items);
                }
                elem => {
                    let key = match elem {
                        Elem::Field { index: i, .. } => int(*i as i64),
                        Elem::Index(i) => i.clone(),
                    };
                    if k == 0 {
                        out.push(stmt(K::Assign(Target::Index(slot.clone(), key), new_value)));
                        return;
                    }
                    let t = self.temp();
                    out.push(stmt(K::Assign(Target::Name(t.clone()), container)));
                    out.push(stmt(K::Assign(Target::Index(t.clone(), key), new_value)));
                    new_value = name(&t);
                }
            }
        }
        out.push(stmt(K::Assign(Target::Name(slot), new_value)));
    }

    // ----- Types -----

    fn expect(&mut self, expected: &Ty, got: &Ty, line: u32) -> R<()> {
        if self.types.unify(expected, got) {
            return Ok(());
        }
        Err(self.mismatch(expected, got, line))
    }

    fn mismatch(&self, expected: &Ty, got: &Ty, line: u32) -> ScriptError {
        let e = self.types.display(expected);
        let g = self.types.display(got);
        let (fr_hint, en_hint) = match (self.types.shallow(expected), self.types.shallow(got)) {
            (Ty::Ref(false, _), other) if !matches!(other, Ty::Ref(..)) => (
                " — prête la valeur avec & (par exemple &x)",
                " — lend the value with & (for example &x)",
            ),
            (Ty::Ref(true, _), _) => (
                " — prête-la en écriture avec &mut (par exemple &mut x)",
                " — lend it mutably with &mut (for example &mut x)",
            ),
            (other, Ty::Ref(..)) if !matches!(other, Ty::Ref(..)) => (
                " — pour lire la valeur derrière la référence, écris *x",
                " — to read the value behind the reference, write *x",
            ),
            (Ty::String, Ty::Str) => (
                " — transforme le texte avec .to_string()",
                " — turn the text into a String with .to_string()",
            ),
            _ => ("", ""),
        };
        rerr(
            line,
            "E0308",
            format!("types incompatibles : « {e} » était attendu, mais c'est « {g} »{fr_hint}"),
            format!("mismatched types: expected '{e}', found '{g}'{en_hint}"),
        )
    }

    fn expect_int(&mut self, ty: &Ty, line: u32, what: &str) -> R<()> {
        let stripped = self.types.strip(ty);
        if self.types.unify(&stripped, &Ty::Int) {
            return Ok(());
        }
        Err(rerr(
            line,
            "E0308",
            format!(
                "{what} attend un nombre entier, pas « {} »",
                self.types.display(ty)
            ),
            format!(
                "{what} expects an integer, not '{}'",
                self.types.display(ty)
            ),
        ))
    }

    fn expect_bool(&mut self, ty: &Ty, line: u32) -> R<()> {
        let stripped = self.types.strip(ty);
        if self.types.unify(&stripped, &Ty::Bool) {
            return Ok(());
        }
        Err(rerr(
            line,
            "E0308",
            format!(
                "une condition doit être un booléen (true / false), pas « {} »",
                self.types.display(ty)
            ),
            format!(
                "a condition must be a boolean (true / false), not '{}'",
                self.types.display(ty)
            ),
        ))
    }

    // ----- Instructions -----

    fn block(&mut self, b: &Block, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        self.push_scope();
        for stmt in &b.stmts {
            self.stmt(stmt, out)?;
        }
        let result = match &b.tail {
            Some(e) => self.value(e, out)?,
            None => (
                unit(),
                if self.f.flow.reachable {
                    Ty::Unit
                } else {
                    Ty::Never
                },
            ),
        };
        let result = if !self.f.flow.reachable {
            (unit(), Ty::Never)
        } else {
            result
        };
        self.pop_scope();
        Ok(result)
    }

    fn stmt(&mut self, stmt: &Stmt, out: &mut Vec<CStmt>) -> R<()> {
        match stmt {
            Stmt::Expr(e) => {
                let (v, _) = self.expr(e, out)?;
                self.discard(v, e.line, out);
                Ok(())
            }
            Stmt::Let {
                pat,
                ty,
                init,
                otherwise,
                line,
            } => self.let_stmt(
                pat,
                ty.as_ref(),
                init.as_ref(),
                otherwise.as_ref(),
                *line,
                out,
            ),
        }
    }

    fn discard(&mut self, v: CExpr, line: u32, out: &mut Vec<CStmt>) {
        if !is_simple(&v) {
            out.push(CStmt {
                line,
                kind: K::Expr(v),
            });
        }
    }

    fn let_stmt(
        &mut self,
        pat: &Pat,
        ty: Option<&TypeExpr>,
        init: Option<&Expr>,
        otherwise: Option<&Block>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<()> {
        let annotation = ty.map(|t| self.ty(t, line)).transpose()?;
        let Some(init) = init else {
            let Pat::Bind {
                name: var, mutable, ..
            } = pat
            else {
                return Err(unsupported(
                    line,
                    "« let » sans valeur avec un motif",
                    "'let' without a value and with a pattern",
                ));
            };
            let ty = annotation.unwrap_or_else(|| self.types.fresh());
            self.declare(var, ty, *mutable);
            let id = self.f.locals.len() - 1;
            self.f.flow.uninit.insert(id);
            return Ok(());
        };
        if matches!(pat, Pat::Wild) {
            // `let _ = expr;` : évalue sans rien garder (et sans déplacer).
            let (v, t) = self.expr(init, out)?;
            if let Some(a) = &annotation {
                self.expect(a, &t, line)?;
            }
            self.discard(v, line, out);
            return Ok(());
        }
        let (v, vty) = self.value(init, out)?;
        let vty = match annotation {
            Some(a) => {
                self.expect(&a, &vty, init.line)?;
                a
            }
            None => vty,
        };
        if matches!(self.types.shallow(&vty), Ty::Ref(true, _)) {
            return Err(unsupported(
                line,
                "garder une référence &mut dans une variable : passe-la directement en argument, f(&mut x)",
                "keeping a &mut reference in a variable: pass it directly as an argument, f(&mut x)",
            ));
        }
        if let (
            Pat::Bind {
                name: var,
                mutable,
                by_ref: false,
            },
            None,
        ) = (pat, otherwise)
            && !self.is_const_pattern(var)
        {
            let slot = self.declare(var, vty, *mutable);
            out.push(CStmt {
                line,
                kind: K::Assign(Target::Name(slot), v),
            });
            return Ok(());
        }
        let scrut = if is_simple(&v) {
            v
        } else {
            let t = self.temp();
            out.push(CStmt {
                line,
                kind: K::Assign(Target::Name(t.clone()), v),
            });
            name(&t)
        };
        let mut binds = Vec::new();
        let test = self.pattern(pat, &scrut, &vty, &mut binds, None, false, line)?;
        match (test, otherwise) {
            (Some(test), Some(otherwise)) => {
                let saved = self.f.flow.clone();
                let mut else_out = Vec::new();
                let (_, else_ty) = self.block(otherwise, &mut else_out)?;
                if !matches!(self.types.shallow(&else_ty), Ty::Never) {
                    return Err(rerr(
                        otherwise.line,
                        "E0308",
                        "le bloc else de « let … else » doit sortir (return, break, continue ou panic!)",
                        "the else block of 'let … else' must leave (return, break, continue or panic!)",
                    ));
                }
                self.f.flow = saved;
                out.push(CStmt {
                    line,
                    kind: K::If(vec![(not(test), else_out)], Vec::new()),
                });
            }
            (Some(_), None) => {
                return Err(rerr(
                    line,
                    "E0005",
                    "ce motif peut ne pas correspondre (par exemple None) : utilise if let, match ou let … else",
                    "this pattern may not match (for example None): use if let, match or let … else",
                ));
            }
            (None, _) => {}
        }
        self.bind(binds, None, line, out)
    }

    fn is_const_pattern(&self, var: &str) -> bool {
        is_upper(var) && self.consts.contains_key(var)
    }

    /// Crée les variables d'un motif.
    fn bind(
        &mut self,
        binds: Vec<Binding>,
        scrutinee: Option<&Place>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<()> {
        let mut moved = false;
        for b in binds {
            if b.by_value && !self.types.is_copy(&b.ty) {
                if b.behind_ref {
                    let shown = self.types.display(&b.ty);
                    return Err(rerr(
                        line,
                        "E0507",
                        format!(
                            "impossible de prendre « {} » (« {shown} ») : la valeur est seulement empruntée. Utilise ref {} ou .clone()",
                            b.name, b.name
                        ),
                        format!(
                            "cannot take '{}' ('{shown}'): the value is only borrowed. Use ref {} or .clone()",
                            b.name, b.name
                        ),
                    ));
                }
                if let Some(place) = scrutinee
                    && !moved
                {
                    let mut whole = place.clone();
                    whole.ty = b.ty.clone();
                    self.do_move(&whole, line)?;
                    moved = true;
                }
            }
            let slot = self.declare(&b.name, b.ty, b.mutable);
            out.push(CStmt {
                line,
                kind: K::Assign(Target::Name(slot), b.value),
            });
        }
        Ok(())
    }

    // ----- Motifs -----

    #[allow(clippy::too_many_arguments)]
    fn pattern(
        &mut self,
        pat: &Pat,
        scrut: &CExpr,
        ty: &Ty,
        binds: &mut Vec<Binding>,
        ref_mode: Option<bool>,
        behind_ref: bool,
        line: u32,
    ) -> R<Option<CExpr>> {
        let shallow = self.types.shallow(ty);
        if let Ty::Ref(m, inner) = &shallow
            && !matches!(pat, Pat::Bind { .. } | Pat::Wild | Pat::Ref(_))
        {
            let mode = Some(ref_mode.map_or(*m, |r| r && *m));
            return self.pattern(pat, scrut, inner, binds, mode, behind_ref, line);
        }
        let mismatch = |l: &Self, expected: &str| {
            rerr(
                line,
                "E0308",
                format!(
                    "ce motif attend {expected}, mais la valeur est de type « {} »",
                    l.types.display(ty)
                ),
                format!(
                    "this pattern expects {expected}, but the value has type '{}'",
                    l.types.display(ty)
                ),
            )
        };
        Ok(match pat {
            Pat::Wild => None,
            Pat::Bind {
                name: var,
                mutable,
                by_ref,
            } => {
                if self.is_const_pattern(var) {
                    let cty = self.consts[var].clone();
                    self.expect(&cty, &shallow, line)?;
                    return Ok(Some(bin(CBin::Eq, scrut.clone(), name(var))));
                }
                let (bty, by_value) = match (by_ref, ref_mode) {
                    (true, _) => (Ty::Ref(false, Box::new(ty.clone())), false),
                    (false, Some(m)) => (Ty::Ref(m, Box::new(ty.clone())), false),
                    (false, None) => (ty.clone(), true),
                };
                binds.push(Binding {
                    name: var.clone(),
                    mutable: *mutable,
                    ty: bty,
                    value: scrut.clone(),
                    by_value,
                    behind_ref,
                });
                None
            }
            Pat::Int(n) => {
                if !self.types.unify(&shallow, &Ty::Int) {
                    return Err(mismatch(self, "un nombre / a number"));
                }
                Some(bin(CBin::Eq, scrut.clone(), int(*n)))
            }
            Pat::Str(s) => {
                if !matches!(self.types.shallow(&shallow), Ty::String)
                    && !self.types.unify(&shallow, &Ty::Str)
                {
                    return Err(mismatch(self, "un texte / a text"));
                }
                Some(bin(CBin::Eq, scrut.clone(), CExpr::Str(s.clone())))
            }
            Pat::Bool(b) => {
                if !self.types.unify(&shallow, &Ty::Bool) {
                    return Err(mismatch(self, "un booléen / a boolean"));
                }
                Some(bin(CBin::Eq, scrut.clone(), CExpr::Bool(*b)))
            }
            Pat::Range(a, b, inclusive) => {
                if !self.types.unify(&shallow, &Ty::Int) {
                    return Err(mismatch(self, "un nombre / a number"));
                }
                let upper = if *inclusive { CBin::Le } else { CBin::Lt };
                and_all(vec![
                    bin(CBin::Ge, scrut.clone(), int(*a)),
                    bin(upper, scrut.clone(), int(*b)),
                ])
            }
            Pat::Tuple(items) if items.is_empty() => {
                if !self.types.unify(&shallow, &Ty::Unit) {
                    return Err(mismatch(self, "()"));
                }
                None
            }
            Pat::Tuple(items) => {
                let tys = match self.types.shallow(&shallow) {
                    Ty::Tuple(tys) if tys.len() == items.len() => tys,
                    Ty::Var(_) => {
                        let tys: Vec<Ty> = items.iter().map(|_| self.types.fresh()).collect();
                        self.types.unify(&shallow, &Ty::Tuple(tys.clone()));
                        tys
                    }
                    _ => {
                        return Err(mismatch(
                            self,
                            &format!(
                                "un tuple de {} éléments / a tuple of {} items",
                                items.len(),
                                items.len()
                            ),
                        ));
                    }
                };
                let mut tests = Vec::new();
                for (i, (p, t)) in items.iter().zip(&tys).enumerate() {
                    let sub = index(scrut.clone(), int(i as i64));
                    if let Some(test) =
                        self.pattern(p, &sub, t, binds, ref_mode, behind_ref, line)?
                    {
                        tests.push(test);
                    }
                }
                and_all(tests)
            }
            Pat::Path(path) => {
                if path.len() == 1 && path[0] == "None" {
                    let inner = self.types.fresh();
                    if !self.types.unify(&shallow, &Ty::Option(Box::new(inner))) {
                        return Err(mismatch(self, "une Option / an Option"));
                    }
                    return Ok(Some(bin(CBin::Eq, scrut.clone(), CExpr::None)));
                }
                let (adt, variant, payload) = self.variant(path, line)?;
                if !self.types.unify(&shallow, &Ty::Adt(adt)) {
                    return Err(mismatch(
                        self,
                        &format!("« {} »", self.types.adts[adt].name),
                    ));
                }
                match variant {
                    Variant::Game(value) => Some(bin(CBin::Eq, scrut.clone(), CExpr::Const(value))),
                    Variant::User(v) => {
                        if !payload.is_empty() {
                            return Err(rerr(
                                line,
                                "E0532",
                                format!("la variante « {v} » contient des données : écris {v}(..)"),
                                format!("the variant '{v}' holds data: write {v}(..)"),
                            ));
                        }
                        Some(bin(CBin::Eq, index(scrut.clone(), int(0)), CExpr::Str(v)))
                    }
                }
            }
            Pat::TupleStruct(path, items) => {
                let single = |items: &[Pat], what: &str| -> R<()> {
                    if items.len() != 1 {
                        return Err(rerr(
                            line,
                            "E0023",
                            format!("{what}(…) contient une seule valeur"),
                            format!("{what}(…) holds a single value"),
                        ));
                    }
                    Ok(())
                };
                if path.len() == 1 && path[0] == "Some" {
                    single(items, "Some")?;
                    let inner = self.types.fresh();
                    if !self
                        .types
                        .unify(&shallow, &Ty::Option(Box::new(inner.clone())))
                    {
                        return Err(mismatch(self, "une Option / an Option"));
                    }
                    let mut tests = vec![bin(CBin::Ne, scrut.clone(), CExpr::None)];
                    tests.extend(
                        self.pattern(&items[0], scrut, &inner, binds, ref_mode, behind_ref, line)?,
                    );
                    return Ok(and_all(tests));
                }
                if path.len() == 1 && (path[0] == "Ok" || path[0] == "Err") {
                    single(items, &path[0])?;
                    let (ok, err) = (self.types.fresh(), self.types.fresh());
                    if !self.types.unify(
                        &shallow,
                        &Ty::Result(Box::new(ok.clone()), Box::new(err.clone())),
                    ) {
                        return Err(mismatch(self, "un Result / a Result"));
                    }
                    let flag = index(scrut.clone(), int(0));
                    let (test, inner) = if path[0] == "Ok" {
                        (flag, ok)
                    } else {
                        (not(flag), err)
                    };
                    let mut tests = vec![test];
                    let sub = index(scrut.clone(), int(1));
                    tests.extend(
                        self.pattern(&items[0], &sub, &inner, binds, ref_mode, behind_ref, line)?,
                    );
                    return Ok(and_all(tests));
                }
                let (adt, variant, payload) = self.variant(path, line)?;
                if !self.types.unify(&shallow, &Ty::Adt(adt)) {
                    return Err(mismatch(
                        self,
                        &format!("« {} »", self.types.adts[adt].name),
                    ));
                }
                let Variant::User(v) = variant else {
                    return Err(rerr(
                        line,
                        "E0532",
                        "cette variante ne contient pas de données",
                        "this variant holds no data",
                    ));
                };
                if payload.len() != items.len() {
                    return Err(rerr(
                        line,
                        "E0023",
                        format!(
                            "la variante « {v} » contient {} valeur(s), le motif en a {}",
                            payload.len(),
                            items.len()
                        ),
                        format!(
                            "the variant '{v}' holds {} value(s), the pattern has {}",
                            payload.len(),
                            items.len()
                        ),
                    ));
                }
                let mut tests = vec![bin(CBin::Eq, index(scrut.clone(), int(0)), CExpr::Str(v))];
                for (i, (p, t)) in items.iter().zip(&payload).enumerate() {
                    let sub = index(scrut.clone(), int(i as i64 + 1));
                    tests.extend(self.pattern(p, &sub, t, binds, ref_mode, behind_ref, line)?);
                }
                and_all(tests)
            }
            Pat::Struct(path, fields) => {
                let ty_name = path.last().cloned().unwrap_or_default();
                let sty = if ty_name == "Self" {
                    self.self_ty.clone()
                } else {
                    self.type_names.get(&ty_name).cloned()
                };
                let Some(sty) = sty else {
                    return Err(self.unknown_type(&ty_name, line));
                };
                if !self.types.unify(&shallow, &sty) {
                    return Err(mismatch(self, &format!("« {ty_name} »")));
                }
                let mut tests = Vec::new();
                for (f, p) in fields {
                    let (i, fty, _) = self.field(&sty, f, line)?;
                    let sub = index(scrut.clone(), int(i as i64));
                    tests.extend(self.pattern(p, &sub, &fty, binds, ref_mode, behind_ref, line)?);
                }
                and_all(tests)
            }
            Pat::Or(options) => {
                let mut tests = Vec::new();
                let mut always = false;
                for p in options {
                    let mut sub_binds = Vec::new();
                    match self.pattern(p, scrut, ty, &mut sub_binds, ref_mode, behind_ref, line)? {
                        Some(t) => tests.push(t),
                        None => always = true,
                    }
                    if !sub_binds.is_empty() {
                        return Err(unsupported(
                            line,
                            "les variables dans un motif « a | b »",
                            "variables inside an 'a | b' pattern",
                        ));
                    }
                }
                if always {
                    None
                } else {
                    tests
                        .into_iter()
                        .reduce(|a, b| CExpr::Or(Box::new(a), Box::new(b)))
                }
            }
            Pat::Ref(inner) => match self.types.shallow(&shallow) {
                Ty::Ref(_, t) => self.pattern(inner, scrut, &t, binds, None, true, line)?,
                _ => {
                    return Err(mismatch(self, "une référence (&) / a reference (&)"));
                }
            },
        })
    }

    /// `Type::Variante` : énumération, variante (valeur ou nom) et types des données.
    fn variant(&self, path: &[String], line: u32) -> R<(usize, Variant, Vec<Ty>)> {
        if path.len() != 2 {
            return Err(rerr(
                line,
                "E0531",
                format!(
                    "« {} » n'est pas une variante connue (écris Type::Variante)",
                    path.join("::")
                ),
                format!(
                    "'{}' is not a known variant (write Type::Variant)",
                    path.join("::")
                ),
            ));
        }
        let ty = if path[0] == "Self" {
            self.self_ty.clone()
        } else {
            self.type_names.get(&path[0]).cloned()
        };
        let Some(Ty::Adt(adt)) = ty else {
            return Err(self.unknown_type(&path[0], line));
        };
        let wanted = &path[1];
        match &self.types.adts[adt].kind {
            AdtKind::Enum { variants } => {
                if let Some((v, payload)) = variants.iter().find(|(v, _)| v == wanted) {
                    return Ok((adt, Variant::User(v.clone()), payload.clone()));
                }
                let suggestion =
                    suggest(wanted, variants.iter().map(|(v, _)| v.as_str()).collect());
                Err(rerr(
                    line,
                    "E0599",
                    format!(
                        "« {} » n'a pas de variante « {wanted} »{}",
                        path[0],
                        hint_fr(&suggestion)
                    ),
                    format!(
                        "'{}' has no variant '{wanted}'{}",
                        path[0],
                        hint_en(&suggestion)
                    ),
                ))
            }
            AdtKind::GameEnum { variants } => {
                if let Some((_, value)) = variants
                    .iter()
                    .find(|(names, _)| names.iter().any(|n| n == wanted))
                {
                    return Ok((adt, Variant::Game(value.clone()), Vec::new()));
                }
                let suggestion = suggest(
                    wanted,
                    variants
                        .iter()
                        .flat_map(|(n, _)| n.iter().map(String::as_str))
                        .collect(),
                );
                Err(rerr(
                    line,
                    "E0599",
                    format!(
                        "« {} » n'a pas de variante « {wanted} »{}",
                        path[0],
                        hint_fr(&suggestion)
                    ),
                    format!(
                        "'{}' has no variant '{wanted}'{}",
                        path[0],
                        hint_en(&suggestion)
                    ),
                ))
            }
            _ => Err(rerr(
                line,
                "E0599",
                format!("« {} » n'est pas une énumération", path[0]),
                format!("'{}' is not an enum", path[0]),
            )),
        }
    }

    // ----- Exhaustivité des match -----

    fn exhaustive(&self, rows: Vec<Vec<&Pat>>, tys: &[Ty]) -> bool {
        if tys.is_empty() {
            return !rows.is_empty();
        }
        // Développe les motifs « a | b » de la première colonne.
        let mut expanded: Vec<Vec<&Pat>> = Vec::new();
        for row in rows {
            let mut stack = vec![row];
            while let Some(row) = stack.pop() {
                match row[0] {
                    Pat::Or(options) => {
                        for p in options {
                            let mut r = row.clone();
                            r[0] = p;
                            stack.push(r);
                        }
                    }
                    Pat::Ref(inner) => {
                        let mut r = row.clone();
                        r[0] = inner;
                        stack.push(r);
                    }
                    _ => expanded.push(row),
                }
            }
        }
        let ty = self.types.strip(&tys[0]);
        let rest = &tys[1..];
        match self.constructors(&ty) {
            Some(ctors) => ctors.into_iter().all(|(ctor, arg_tys)| {
                let rows: Vec<Vec<&Pat>> = expanded
                    .iter()
                    .filter_map(|row| {
                        let mut subs = self.specialize(row[0], &ctor, arg_tys.len(), &ty)?;
                        subs.extend(row[1..].iter().copied());
                        Some(subs)
                    })
                    .collect();
                let mut tys2 = arg_tys.clone();
                tys2.extend(rest.iter().cloned());
                self.exhaustive(rows, &tys2)
            }),
            None => {
                let rows: Vec<Vec<&Pat>> = expanded
                    .iter()
                    .filter(|row| self.is_wild(row[0]))
                    .map(|row| row[1..].to_vec())
                    .collect();
                self.exhaustive(rows, rest)
            }
        }
    }

    fn is_wild(&self, pat: &Pat) -> bool {
        match pat {
            Pat::Wild => true,
            Pat::Bind { name, .. } => !self.is_const_pattern(name),
            _ => false,
        }
    }

    fn constructors(&self, ty: &Ty) -> Option<Vec<(Ctor, Vec<Ty>)>> {
        Some(match ty {
            Ty::Bool => vec![(Ctor::Bool(true), vec![]), (Ctor::Bool(false), vec![])],
            Ty::Unit => vec![(Ctor::Single, vec![])],
            Ty::Option(t) => vec![(Ctor::Some, vec![(**t).clone()]), (Ctor::None, vec![])],
            Ty::Result(t, e) => vec![
                (Ctor::Ok, vec![(**t).clone()]),
                (Ctor::Err, vec![(**e).clone()]),
            ],
            Ty::Tuple(items) => vec![(Ctor::Single, items.clone())],
            Ty::Adt(i) => match &self.types.adts[*i].kind {
                AdtKind::Struct { fields } => vec![(
                    Ctor::Single,
                    fields.iter().map(|(_, t)| t.clone()).collect(),
                )],
                AdtKind::Enum { variants } => variants
                    .iter()
                    .map(|(v, payload)| (Ctor::Variant(v.clone()), payload.clone()))
                    .collect(),
                AdtKind::GameEnum { variants } => variants
                    .iter()
                    .map(|(names, _)| (Ctor::Variant(names[0].clone()), Vec::new()))
                    .collect(),
                AdtKind::Opaque { .. } => return None,
            },
            _ => return None,
        })
    }

    /// Les sous-motifs de `pat` s'il correspond au constructeur `ctor`.
    fn specialize<'p>(
        &self,
        pat: &'p Pat,
        ctor: &Ctor,
        arity: usize,
        ty: &Ty,
    ) -> Option<Vec<&'p Pat>> {
        if self.is_wild(pat) {
            return Some(vec![&WILD; arity]);
        }
        let same_variant = |path: &[String]| -> bool {
            let Ctor::Variant(wanted) = ctor else {
                return false;
            };
            let Some(last) = path.last() else {
                return false;
            };
            match ty {
                Ty::Adt(i) => match &self.types.adts[*i].kind {
                    AdtKind::GameEnum { variants } => variants
                        .iter()
                        .any(|(names, _)| names.contains(wanted) && names.contains(last)),
                    _ => last == wanted,
                },
                _ => false,
            }
        };
        match (pat, ctor) {
            (Pat::Ref(inner), _) => self.specialize(inner, ctor, arity, ty),
            (Pat::Bool(b), Ctor::Bool(c)) if b == c => Some(vec![]),
            (Pat::Path(p), Ctor::None) if p.len() == 1 && p[0] == "None" => Some(vec![]),
            (Pat::TupleStruct(p, items), Ctor::Some) if p.len() == 1 && p[0] == "Some" => {
                Some(items.iter().collect())
            }
            (Pat::TupleStruct(p, items), Ctor::Ok) if p.len() == 1 && p[0] == "Ok" => {
                Some(items.iter().collect())
            }
            (Pat::TupleStruct(p, items), Ctor::Err) if p.len() == 1 && p[0] == "Err" => {
                Some(items.iter().collect())
            }
            (Pat::Path(p), Ctor::Variant(_)) if same_variant(p) => Some(vec![]),
            (Pat::TupleStruct(p, items), Ctor::Variant(_)) if same_variant(p) => {
                Some(items.iter().collect())
            }
            (Pat::Tuple(items), Ctor::Single) => Some(items.iter().collect()),
            (Pat::Struct(_, fields), Ctor::Single) => {
                let Ty::Adt(i) = ty else { return None };
                let AdtKind::Struct { fields: decl } = &self.types.adts[*i].kind else {
                    return None;
                };
                Some(
                    decl.iter()
                        .map(|(f, _)| {
                            fields
                                .iter()
                                .find(|(n, _)| n == f)
                                .map_or(&WILD, |(_, p)| p)
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    // ----- Expressions -----

    /// Évalue une expression en **prenant** sa valeur : une variable non copiable est
    /// déplacée.
    fn value(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        if let Some(place) = self.place(e, out)? {
            self.check_use(&place, e.line)?;
            self.do_move(&place, e.line)?;
            return Ok((self.place_expr(&place), place.ty.clone()));
        }
        self.expr_inner(e, out)
    }

    /// Évalue une expression en la **lisant** seulement (comparaison, affichage…).
    fn expr(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        if let Some(place) = self.place(e, out)? {
            self.check_use(&place, e.line)?;
            return Ok((self.place_expr(&place), place.ty.clone()));
        }
        self.expr_inner(e, out)
    }

    /// Plusieurs sous-expressions, dans l'ordre : si l'une a besoin d'instructions
    /// préalables, celles d'avant sont d'abord rangées dans des temporaires.
    fn sequence(
        &mut self,
        parts: Vec<(Vec<CStmt>, CExpr)>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> Vec<CExpr> {
        let mut exprs: Vec<CExpr> = Vec::new();
        for (pre, e) in parts {
            if !pre.is_empty() {
                for previous in &mut exprs {
                    if !is_const(previous) {
                        let t = self.temp();
                        out.push(CStmt {
                            line,
                            kind: K::Assign(
                                Target::Name(t.clone()),
                                mem::replace(previous, name(&t)),
                            ),
                        });
                    }
                }
            }
            out.extend(pre);
            exprs.push(e);
        }
        exprs
    }

    fn values(
        &mut self,
        items: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(Vec<CExpr>, Vec<Ty>)> {
        let mut parts = Vec::new();
        let mut tys = Vec::new();
        for item in items {
            let mut pre = Vec::new();
            let (v, t) = self.value(item, &mut pre)?;
            parts.push((pre, v));
            tys.push(t);
        }
        Ok((self.sequence(parts, line, out), tys))
    }

    fn spill(&mut self, e: CExpr, line: u32, out: &mut Vec<CStmt>) -> CExpr {
        if is_simple(&e) {
            return e;
        }
        let t = self.temp();
        out.push(CStmt {
            line,
            kind: K::Assign(Target::Name(t.clone()), e),
        });
        name(&t)
    }

    fn expr_inner(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        let line = e.line;
        match &e.kind {
            ExprKind::Int(n) => Ok((int(*n), Ty::Int)),
            ExprKind::Str(s) => Ok((CExpr::Str(s.clone()), Ty::Str)),
            ExprKind::Bool(b) => Ok((CExpr::Bool(*b), Ty::Bool)),
            ExprKind::Path(path) => self.path(path, line),
            ExprKind::Tuple(items) if items.is_empty() => Ok((unit(), Ty::Unit)),
            ExprKind::Tuple(items) => {
                let (vs, tys) = self.values(items, line, out)?;
                Ok((CExpr::Tuple(vs), Ty::Tuple(tys)))
            }
            ExprKind::Array(items) => {
                let (vs, tys) = self.values(items, line, out)?;
                let elem = self.types.fresh();
                for (t, item) in tys.iter().zip(items) {
                    self.expect(&elem, t, item.line)?;
                }
                Ok((CExpr::List(vs), Ty::Vec(Box::new(elem))))
            }
            ExprKind::ArrayRepeat(item, count) => {
                let (vs, tys) = self.values(&[(**item).clone(), (**count).clone()], line, out)?;
                self.expect_int(&tys[1], line, "vec![x; n]")?;
                if !self.types.is_clone(&tys[0]) {
                    return Err(rerr(
                        line,
                        "E0277",
                        format!(
                            "vec![x; n] copie x, mais « {} » n'est pas Clone",
                            self.types.display(&tys[0])
                        ),
                        format!(
                            "vec![x; n] copies x, but '{}' is not Clone",
                            self.types.display(&tys[0])
                        ),
                    ));
                }
                let mut vs = vs.into_iter();
                let (x, n) = (vs.next().unwrap(), vs.next().unwrap());
                Ok((
                    bin(CBin::Mul, CExpr::List(vec![x]), n),
                    Ty::Vec(Box::new(tys[0].clone())),
                ))
            }
            ExprKind::StructLit(path, fields) => self.struct_literal(path, fields, line, out),
            ExprKind::Unary(op, inner) => {
                let (v, t) = self.value(inner, out)?;
                match op {
                    UnOp::Neg => {
                        self.expect_int(&t, line, "« - »")?;
                        Ok((CExpr::Neg(Box::new(v)), Ty::Int))
                    }
                    UnOp::Not => {
                        let stripped = self.types.strip(&t);
                        if !self.types.unify(&stripped, &Ty::Bool) {
                            return Err(rerr(
                                line,
                                "E0600",
                                format!(
                                    "« ! » s'utilise sur un booléen, pas sur « {} »",
                                    self.types.display(&t)
                                ),
                                format!(
                                    "'!' is used on a boolean, not on '{}'",
                                    self.types.display(&t)
                                ),
                            ));
                        }
                        Ok((not(v), Ty::Bool))
                    }
                    UnOp::Deref => match self.types.shallow(&t) {
                        Ty::Ref(_, inner_ty) => Ok((v, *inner_ty)),
                        other => Err(rerr(
                            line,
                            "E0614",
                            format!(
                                "« * » s'utilise sur une référence, pas sur « {} »",
                                self.types.display(&other)
                            ),
                            format!(
                                "'*' is used on a reference, not on '{}'",
                                self.types.display(&other)
                            ),
                        )),
                    },
                }
            }
            ExprKind::Ref(mutable, inner) => {
                if let Some(mut place) = self.place(inner, out)? {
                    self.check_use(&place, line)?;
                    if *mutable {
                        let mut deref = place.clone();
                        deref.ty = place.ty.clone();
                        self.check_mutable(&deref, line, true)?;
                    }
                    let ty = mem::replace(&mut place.ty, Ty::Unit);
                    return Ok((self.place_expr(&place), Ty::Ref(*mutable, Box::new(ty))));
                }
                let (v, t) = self.value(inner, out)?;
                Ok((v, Ty::Ref(*mutable, Box::new(t))))
            }
            ExprKind::Binary(op, a, b) => self.binary(*op, a, b, line, out),
            ExprKind::Assign(target, value) => {
                if let ExprKind::Path(p) = &target.kind
                    && p.len() == 1
                    && p[0] == "_"
                {
                    let (v, _) = self.expr(value, out)?;
                    self.discard(v, line, out);
                    return Ok((unit(), Ty::Unit));
                }
                let (v, vt) = self.value(value, out)?;
                let Some(place) = self.place(target, out)? else {
                    return Err(rerr(
                        line,
                        "E0070",
                        "on ne peut donner une valeur qu'à une variable, un champ ou un élément [i]",
                        "only a variable, a field or an item [i] can be assigned",
                    ));
                };
                self.check_mutable(&place, line, false)?;
                if !place.elems.is_empty() {
                    self.check_use(&place, line)?;
                }
                self.expect(&place.ty, &vt, line)?;
                self.store(&place, v, line, out);
                self.reinit(&place);
                Ok((unit(), Ty::Unit))
            }
            ExprKind::CompoundAssign(op, target, value) => {
                let (v, vt) = self.value(value, out)?;
                let Some(place) = self.place(target, out)? else {
                    return Err(rerr(
                        line,
                        "E0067",
                        "on ne peut modifier qu'une variable, un champ ou un élément [i]",
                        "only a variable, a field or an item [i] can be modified",
                    ));
                };
                self.check_mutable(&place, line, false)?;
                self.check_use(&place, line)?;
                let current = self.place_expr(&place);
                let combined = match (self.types.strip(&place.ty), op) {
                    (Ty::String, BinOp::Add) => {
                        self.string_operand(&vt, line)?;
                        bin(CBin::Add, current, v)
                    }
                    _ => {
                        self.expect_int(&place.ty, line, &format!("« {}= »", op.symbol()))?;
                        self.expect_int(&vt, line, &format!("« {}= »", op.symbol()))?;
                        bin(arith(*op), current, v)
                    }
                };
                self.store(&place, combined, line, out);
                Ok((unit(), Ty::Unit))
            }
            ExprKind::Cast(inner, target) => {
                let (v, t) = self.value(inner, out)?;
                let target_ty = self.ty(target, line)?;
                match (self.types.strip(&t), self.types.shallow(&target_ty)) {
                    (Ty::Int, Ty::Int) => Ok((v, Ty::Int)),
                    (Ty::Bool, Ty::Int) => Ok((bin(CBin::Add, v, int(0)), Ty::Int)),
                    (Ty::Var(_), Ty::Int) => {
                        let stripped = self.types.strip(&t);
                        self.types.unify(&stripped, &Ty::Int);
                        Ok((v, Ty::Int))
                    }
                    (from, to) => Err(rerr(
                        line,
                        "E0605",
                        format!(
                            "impossible de convertir « {} » en « {} » avec as",
                            self.types.display(&from),
                            self.types.display(&to)
                        ),
                        format!(
                            "cannot convert '{}' into '{}' with as",
                            self.types.display(&from),
                            self.types.display(&to)
                        ),
                    )),
                }
            }
            ExprKind::Call(callee, args) => self.call(callee, args, line, out),
            ExprKind::MethodCall(receiver, method, args) => {
                self.method(receiver, method, args, line, out)
            }
            ExprKind::Field(base, field) => {
                let (v, t) = self.value(base, out)?;
                let (i, fty, _) = self.field(&t, field, line)?;
                Ok((index(v, int(i as i64)), fty))
            }
            ExprKind::Index(base, idx) => {
                let (vs, tys) = self.values(&[(**base).clone(), (**idx).clone()], line, out)?;
                let elem = self.index_elem_ty(&tys[0], line)?;
                self.expect_int(&tys[1], line, "[ ]")?;
                if !self.types.is_copy(&elem) {
                    return Err(rerr(
                        line,
                        "E0507",
                        "impossible de sortir une valeur d'un Vec avec [ ] : utilise .remove(i), &v[i] ou .clone()",
                        "cannot move a value out of a Vec with [ ]: use .remove(i), &v[i] or .clone()",
                    ));
                }
                let mut vs = vs.into_iter();
                Ok((index(vs.next().unwrap(), vs.next().unwrap()), elem))
            }
            ExprKind::Range(start, end, inclusive) => {
                let (Some(start), Some(end)) = (start, end) else {
                    return Err(unsupported(
                        line,
                        "les intervalles ouverts (a.. ou ..b)",
                        "open ranges (a.. or ..b)",
                    ));
                };
                let (vs, tys) = self.values(&[(**start).clone(), (**end).clone()], line, out)?;
                self.expect_int(&tys[0], line, "a..b")?;
                self.expect_int(&tys[1], line, "a..b")?;
                let mut vs = vs.into_iter();
                let (a, mut b) = (vs.next().unwrap(), vs.next().unwrap());
                if *inclusive {
                    b = bin(CBin::Add, b, int(1));
                }
                Ok((CExpr::BuiltinCall(Builtin::Range, vec![a, b]), Ty::Range))
            }
            ExprKind::Try(inner) => self.try_operator(inner, line, out),
            ExprKind::If(condition, then, otherwise) => {
                self.if_expr(condition, then, otherwise.as_deref(), line, out)
            }
            ExprKind::IfLet(pat, scrutinee, then, otherwise) => {
                self.if_let(pat, scrutinee, then, otherwise.as_deref(), line, out)
            }
            ExprKind::Match(scrutinee, arms) => self.match_expr(scrutinee, arms, line, out),
            ExprKind::Block(block) => self.block(block, out),
            ExprKind::Loop(body) => self.loop_expr(None, body, line, out),
            ExprKind::While(condition, body) => {
                self.loop_expr(Some(LoopHead::While(condition)), body, line, out)
            }
            ExprKind::WhileLet(pat, scrutinee, body) => {
                self.loop_expr(Some(LoopHead::WhileLet(pat, scrutinee)), body, line, out)
            }
            ExprKind::For(pat, iterable, body) => self.for_loop(pat, iterable, body, line, out),
            ExprKind::Break(value) => {
                let Some(ctx) = self.f.loops.last() else {
                    return Err(rerr(
                        line,
                        "E0268",
                        "« break » ne s'utilise que dans une boucle",
                        "'break' can only be used inside a loop",
                    ));
                };
                let is_loop = ctx.is_loop;
                if let Some(value) = value {
                    if !is_loop {
                        return Err(rerr(
                            line,
                            "E0571",
                            "« break valeur » ne s'utilise que dans loop (pas dans while ni for)",
                            "'break value' can only be used inside loop (not while or for)",
                        ));
                    }
                    let (v, t) = self.value(value, out)?;
                    let (slot, ty) = match &self.f.loops.last().unwrap().value {
                        Some(existing) => existing.clone(),
                        None => {
                            let slot = self.temp();
                            let ty = self.types.fresh();
                            self.f.loops.last_mut().unwrap().value =
                                Some((slot.clone(), ty.clone()));
                            (slot, ty)
                        }
                    };
                    self.expect(&ty, &t, line)?;
                    out.push(CStmt {
                        line,
                        kind: K::Assign(Target::Name(slot), v),
                    });
                }
                let ctx = self.f.loops.last_mut().unwrap();
                out.extend(ctx.writeback.iter().cloned());
                let flow = mem::replace(&mut self.f.flow, Flow::unreachable());
                self.f.loops.last_mut().unwrap().breaks.push(flow);
                out.push(CStmt {
                    line,
                    kind: K::Break,
                });
                Ok((unit(), Ty::Never))
            }
            ExprKind::Continue => {
                let Some(ctx) = self.f.loops.last() else {
                    return Err(rerr(
                        line,
                        "E0268",
                        "« continue » ne s'utilise que dans une boucle",
                        "'continue' can only be used inside a loop",
                    ));
                };
                out.extend(ctx.writeback.iter().cloned());
                let flow = mem::replace(&mut self.f.flow, Flow::unreachable());
                self.f.loops.last_mut().unwrap().continues.push(flow);
                out.push(CStmt {
                    line,
                    kind: K::Continue,
                });
                Ok((unit(), Ty::Never))
            }
            ExprKind::Return(value) => {
                let (v, t) = match value {
                    Some(value) => self.value(value, out)?,
                    None => (unit(), Ty::Unit),
                };
                let ret = self.f.ret.clone();
                if !self.types.unify(&ret, &t) {
                    return Err(rerr(
                        line,
                        "E0308",
                        format!(
                            "cette fonction renvoie « {} », pas « {} »",
                            self.types.display(&ret),
                            self.types.display(&t)
                        ),
                        format!(
                            "this function returns '{}', not '{}'",
                            self.types.display(&ret),
                            self.types.display(&t)
                        ),
                    ));
                }
                self.return_stmts(v, line, out);
                self.f.flow = Flow::unreachable();
                Ok((unit(), Ty::Never))
            }
            ExprKind::Format(kind, template, args) => {
                self.format_macro(*kind, template, args, line, out)
            }
            ExprKind::Assert(condition, message) => {
                let (c, t) = self.expr(condition, out)?;
                self.expect_bool(&t, line)?;
                let message = match message {
                    Some(m) => self.expr(m, out)?.0,
                    None => CExpr::Str("assertion échouée / assertion failed".into()),
                };
                out.push(CStmt {
                    line,
                    kind: K::If(
                        vec![(
                            not(c),
                            vec![CStmt {
                                line,
                                kind: K::Panic(PanicKind::Message, Some(message)),
                            }],
                        )],
                        Vec::new(),
                    ),
                });
                Ok((unit(), Ty::Unit))
            }
            ExprKind::AssertEq(a, b, equal) => {
                let mut parts = Vec::new();
                let mut tys = Vec::new();
                for side in [a, b] {
                    let mut pre = Vec::new();
                    let (v, t) = self.expr(side, &mut pre)?;
                    parts.push((pre, v));
                    tys.push(t);
                }
                let vs = self.sequence(parts, line, out);
                let (left, right) = (
                    self.spill(vs[0].clone(), line, out),
                    self.spill(vs[1].clone(), line, out),
                );
                let (ta, tb) = (self.types.strip(&tys[0]), self.types.strip(&tys[1]));
                self.expect(&ta, &tb, line)?;
                let desc = CExpr::Const(self.types.descriptor(&ta));
                let shown = |v: CExpr| {
                    CExpr::BuiltinCall(Builtin::Format, vec![v, desc.clone(), CExpr::Bool(true)])
                };
                let message = bin(
                    CBin::Add,
                    bin(
                        CBin::Add,
                        bin(
                            CBin::Add,
                            CExpr::Str("assertion échouée / assertion failed : ".into()),
                            shown(left.clone()),
                        ),
                        CExpr::Str(if *equal { " != " } else { " == " }.into()),
                    ),
                    shown(right.clone()),
                );
                let failed = bin(if *equal { CBin::Ne } else { CBin::Eq }, left, right);
                out.push(CStmt {
                    line,
                    kind: K::If(
                        vec![(
                            failed,
                            vec![CStmt {
                                line,
                                kind: K::Panic(PanicKind::Message, Some(message)),
                            }],
                        )],
                        Vec::new(),
                    ),
                });
                Ok((unit(), Ty::Unit))
            }
        }
    }

    fn path(&mut self, path: &[String], line: u32) -> R<(CExpr, Ty)> {
        if path.len() == 1 {
            let var = &path[0];
            if let Some(ty) = self.consts.get(var) {
                return Ok((name(var), ty.clone()));
            }
            if var == "None" {
                let inner = self.types.fresh();
                return Ok((CExpr::None, Ty::Option(Box::new(inner))));
            }
            if self.fns.contains_key(var) || self.hosts.contains_key(var.as_str()) {
                return Err(rerr(
                    line,
                    "E0423",
                    format!(
                        "« {var} » est une fonction : appelle-la avec des parenthèses, {var}(…)"
                    ),
                    format!("'{var}' is a function: call it with parentheses, {var}(…)"),
                ));
            }
            return Err(self.unknown_name(var, line));
        }
        let (adt, variant, payload) = self.variant(path, line)?;
        match variant {
            Variant::Game(value) => Ok((CExpr::Const(value), Ty::Adt(adt))),
            Variant::User(v) => {
                if !payload.is_empty() {
                    return Err(rerr(
                        line,
                        "E0533",
                        format!(
                            "la variante « {v} » contient des données : écris {}::{v}(…)",
                            path[0]
                        ),
                        format!("the variant '{v}' holds data: write {}::{v}(…)", path[0]),
                    ));
                }
                Ok((CExpr::Tuple(vec![CExpr::Str(v)]), Ty::Adt(adt)))
            }
        }
    }

    fn struct_literal(
        &mut self,
        path: &[String],
        fields: &[(String, Expr)],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let type_name = path.last().cloned().unwrap_or_default();
        let ty = if type_name == "Self" {
            self.self_ty.clone()
        } else {
            self.type_names.get(&type_name).cloned()
        };
        let Some(ty) = ty else {
            return Err(self.unknown_type(&type_name, line));
        };
        let decl = match &ty {
            Ty::Adt(i) => match &self.types.adts[*i].kind {
                AdtKind::Struct { fields } => fields.clone(),
                AdtKind::Opaque { .. } => {
                    return Err(rerr(
                        line,
                        "E0451",
                        format!(
                            "« {type_name} » est un type du jeu : on ne peut pas en fabriquer, seulement en recevoir"
                        ),
                        format!("'{type_name}' is a game type: it cannot be built, only received"),
                    ));
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        if !matches!(&ty, Ty::Adt(i) if matches!(self.types.adts[*i].kind, AdtKind::Struct { .. }))
        {
            return Err(rerr(
                line,
                "E0574",
                format!("« {type_name} » n'est pas une structure"),
                format!("'{type_name}' is not a struct"),
            ));
        }
        let mut seen: Vec<&str> = Vec::new();
        for (f, _) in fields {
            if !decl.iter().any(|(d, _)| d == f) {
                return Err(rerr(
                    line,
                    "E0560",
                    format!("la structure « {type_name} » n'a pas de champ « {f} »"),
                    format!("the struct '{type_name}' has no field '{f}'"),
                ));
            }
            if seen.contains(&f.as_str()) {
                return Err(rerr(
                    line,
                    "E0062",
                    format!("le champ « {f} » est donné deux fois"),
                    format!("the field '{f}' is given twice"),
                ));
            }
            seen.push(f);
        }
        let missing: Vec<&str> = decl
            .iter()
            .map(|(d, _)| d.as_str())
            .filter(|d| !seen.contains(d))
            .collect();
        if !missing.is_empty() {
            return Err(rerr(
                line,
                "E0063",
                format!(
                    "il manque le(s) champ(s) {} dans « {type_name} {{ … }} »",
                    missing.join(", ")
                ),
                format!(
                    "missing field(s) {} in '{type_name} {{ … }}'",
                    missing.join(", ")
                ),
            ));
        }
        let exprs: Vec<Expr> = fields.iter().map(|(_, e)| e.clone()).collect();
        let (vs, tys) = self.values(&exprs, line, out)?;
        let mut ordered = vec![CExpr::None; decl.len()];
        for (((f, e), v), t) in fields.iter().zip(vs).zip(tys) {
            let pos = decl.iter().position(|(d, _)| d == f).unwrap();
            self.expect(&decl[pos].1, &t, e.line)?;
            ordered[pos] = v;
        }
        Ok((CExpr::List(ordered), ty))
    }

    fn string_operand(&mut self, ty: &Ty, line: u32) -> R<()> {
        match self.types.shallow(ty) {
            Ty::Str | Ty::Ref(_, _) if matches!(self.types.strip(ty), Ty::Str | Ty::String) => {
                Ok(())
            }
            Ty::String => Err(rerr(
                line,
                "E0308",
                "on ajoute un &str à un String : écris a + &b (avec &)",
                "a &str is added to a String: write a + &b (with &)",
            )),
            other => Err(self.mismatch(&Ty::Str, &other, line)),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        a: &Expr,
        b: &Expr,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        if matches!(op, BinOp::And | BinOp::Or) {
            let (va, ta) = self.expr(a, out)?;
            self.expect_bool(&ta, a.line)?;
            let saved = self.f.flow.clone();
            let mut pre = Vec::new();
            let (vb, tb) = self.expr(b, &mut pre)?;
            self.expect_bool(&tb, b.line)?;
            // L'opérande de droite n'est pas toujours évalué.
            let right_flow = mem::replace(&mut self.f.flow, saved);
            self.f.flow = merge(self.f.flow.clone(), right_flow);
            if pre.is_empty() {
                let e = if op == BinOp::And {
                    CExpr::And(Box::new(va), Box::new(vb))
                } else {
                    CExpr::Or(Box::new(va), Box::new(vb))
                };
                return Ok((e, Ty::Bool));
            }
            let t = self.temp();
            out.push(CStmt {
                line,
                kind: K::Assign(Target::Name(t.clone()), va),
            });
            pre.push(CStmt {
                line,
                kind: K::Assign(Target::Name(t.clone()), vb),
            });
            let condition = if op == BinOp::And {
                name(&t)
            } else {
                not(name(&t))
            };
            out.push(CStmt {
                line,
                kind: K::If(vec![(condition, pre)], Vec::new()),
            });
            return Ok((name(&t), Ty::Bool));
        }
        let comparison = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        );
        let mut parts = Vec::new();
        let mut tys = Vec::new();
        for side in [a, b] {
            let mut pre = Vec::new();
            // Une comparaison emprunte ses opérandes, une opération les prend.
            let (v, t) = if comparison {
                self.expr(side, &mut pre)?
            } else {
                self.value(side, &mut pre)?
            };
            parts.push((pre, v));
            tys.push(t);
        }
        let vs = self.sequence(parts, line, out);
        let (va, vb) = (vs[0].clone(), vs[1].clone());
        let (ta, tb) = (self.types.strip(&tys[0]), self.types.strip(&tys[1]));
        let shown = |l: &Self| (l.types.display(&tys[0]), l.types.display(&tys[1]));
        if comparison {
            let texts = matches!(
                (self.types.shallow(&ta), self.types.shallow(&tb)),
                (Ty::Str | Ty::String, Ty::Str | Ty::String)
            );
            if !texts && !self.types.unify(&ta, &tb) {
                let (x, y) = shown(self);
                return Err(rerr(
                    line,
                    "E0308",
                    format!("impossible de comparer « {x} » avec « {y} »"),
                    format!("cannot compare '{x}' with '{y}'"),
                ));
            }
            let ordered = matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);
            let ok = if ordered {
                matches!(
                    self.types.shallow(&ta),
                    Ty::Int | Ty::Str | Ty::String | Ty::Var(_) | Ty::Tuple(_)
                )
            } else {
                self.types.has_eq(&ta)
            };
            if !ok {
                let (x, _) = shown(self);
                let (fr, en) = match self.types.shallow(&ta) {
                    Ty::Adt(i) if !ordered => (
                        format!(
                            " : ajoute #[derive(PartialEq)] au type {}",
                            self.types.adts[i].name
                        ),
                        format!(
                            ": add #[derive(PartialEq)] to the type {}",
                            self.types.adts[i].name
                        ),
                    ),
                    _ => (String::new(), String::new()),
                };
                return Err(rerr(
                    line,
                    "E0369",
                    format!("« {} » ne s'utilise pas avec « {x} »{fr}", op.symbol()),
                    format!("'{}' cannot be used with '{x}'{en}", op.symbol()),
                ));
            }
            return Ok((bin(arith(op), va, vb), Ty::Bool));
        }
        if op == BinOp::Add && matches!(self.types.shallow(&tys[0]), Ty::String) {
            self.string_operand(&tys[1], line)?;
            return Ok((bin(CBin::Add, va, vb), Ty::String));
        }
        if op == BinOp::Add && matches!(self.types.shallow(&ta), Ty::Str) {
            return Err(rerr(
                line,
                "E0369",
                "on ne peut pas additionner deux &str : écris a.to_string() + b, ou format!(\"{}{}\", a, b)",
                "two &str cannot be added: write a.to_string() + b, or format!(\"{}{}\", a, b)",
            ));
        }
        let left_int = self.types.unify(&ta, &Ty::Int);
        let right_int = self.types.unify(&tb, &Ty::Int);
        if !left_int || !right_int {
            let (x, y) = shown(self);
            return Err(rerr(
                line,
                "E0369",
                format!(
                    "« {} » ne s'utilise pas entre « {x} » et « {y} »",
                    op.symbol()
                ),
                format!("'{}' cannot be used between '{x}' and '{y}'", op.symbol()),
            ));
        }
        Ok((bin(arith(op), va, vb), Ty::Int))
    }

    // ----- Appels -----

    fn call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let ExprKind::Path(path) = &callee.kind else {
            return Err(unsupported(
                line,
                "appeler autre chose qu'une fonction nommée",
                "calling something other than a named function",
            ));
        };
        if path.len() == 1 {
            let fname = &path[0];
            if let Some(local) = self.lookup(fname) {
                let ty = self.types.display(&self.f.locals[local].ty);
                return Err(rerr(
                    line,
                    "E0618",
                    format!("« {fname} » est une variable (de type « {ty} »), pas une fonction"),
                    format!("'{fname}' is a variable (of type '{ty}'), not a function"),
                ));
            }
            match fname.as_str() {
                "Some" | "Ok" | "Err" => {
                    if args.len() != 1 {
                        return Err(self.arg_count(fname, 1, args.len(), line));
                    }
                    let (v, t) = self.value(&args[0], out)?;
                    return Ok(match fname.as_str() {
                        "Some" => (v, Ty::Option(Box::new(t))),
                        "Ok" => {
                            let e = self.types.fresh();
                            (
                                CExpr::Tuple(vec![CExpr::Bool(true), v]),
                                Ty::Result(Box::new(t), Box::new(e)),
                            )
                        }
                        _ => {
                            let ok = self.types.fresh();
                            (
                                CExpr::Tuple(vec![CExpr::Bool(false), v]),
                                Ty::Result(Box::new(ok), Box::new(t)),
                            )
                        }
                    });
                }
                _ => {}
            }
            if let Some(sig) = self.fns.get(fname).cloned() {
                return self.call_user(&sig, None, args, line, out);
            }
            if let Some(&host) = self.hosts.get(fname.as_str()) {
                return self.call_host(host, fname, args, line, out);
            }
            return Err(self.unknown_name(fname, line));
        }
        if path.len() == 2 {
            let (ty_name, item) = (&path[0], &path[1]);
            match (ty_name.as_str(), item.as_str()) {
                ("Vec", "new") | ("Vec", "with_capacity") => {
                    for a in args {
                        let (v, _) = self.value(a, out)?;
                        self.discard(v, line, out);
                    }
                    let elem = self.types.fresh();
                    return Ok((CExpr::List(Vec::new()), Ty::Vec(Box::new(elem))));
                }
                ("String", "new") => return Ok((CExpr::Str(String::new()), Ty::String)),
                ("String", "from") => {
                    if args.len() != 1 {
                        return Err(self.arg_count("String::from", 1, args.len(), line));
                    }
                    let (v, t) = self.value(&args[0], out)?;
                    if !matches!(self.types.strip(&t), Ty::Str | Ty::String) {
                        return Err(self.mismatch(&Ty::Str, &t, line));
                    }
                    return Ok((v, Ty::String));
                }
                _ => {}
            }
            let ty = if ty_name == "Self" {
                self.self_ty.clone()
            } else {
                self.type_names.get(ty_name).cloned()
            };
            let Some(Ty::Adt(adt)) = ty else {
                return Err(self.unknown_type(ty_name, line));
            };
            if let Some(sig) = self.fns.get(&format!("{adt}::{item}")).cloned() {
                if sig.self_kind.is_some() {
                    return Err(unsupported(
                        line,
                        &format!("l'appel Type::méthode(x) : écris x.{item}(…)"),
                        &format!("the Type::method(x) call: write x.{item}(…)"),
                    ));
                }
                return self.call_user(&sig, None, args, line, out);
            }
            let (adt, variant, payload) = self.variant(path, line)?;
            let Variant::User(v) = variant else {
                return Err(rerr(
                    line,
                    "E0618",
                    "cette variante ne contient pas de données",
                    "this variant holds no data",
                ));
            };
            if payload.len() != args.len() {
                return Err(self.arg_count(
                    &format!("{ty_name}::{v}"),
                    payload.len(),
                    args.len(),
                    line,
                ));
            }
            let (vs, tys) = self.values(args, line, out)?;
            for ((p, t), a) in payload.iter().zip(&tys).zip(args) {
                self.expect(p, t, a.line)?;
            }
            let mut items = vec![CExpr::Str(v)];
            items.extend(vs);
            return Ok((CExpr::Tuple(items), Ty::Adt(adt)));
        }
        Err(self.unknown_name(&path.join("::"), line))
    }

    fn arg_count(&self, fname: &str, expected: usize, got: usize, line: u32) -> ScriptError {
        rerr(
            line,
            "E0061",
            format!("{fname} attend {expected} argument(s), mais en reçoit {got}"),
            format!("{fname} takes {expected} argument(s) but got {got}"),
        )
    }

    fn call_host(
        &mut self,
        host: usize,
        fname: &str,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let function = self.api.functions[host].clone();
        if function.params.len() != args.len() {
            return Err(self.arg_count(fname, function.params.len(), args.len(), line));
        }
        let (vs, tys) = self.values(args, line, out)?;
        for ((p, t), a) in function.params.iter().zip(&tys).zip(args) {
            let expected = self.host_ty(p);
            let got = self.types.strip(t);
            if !self.types.unify(&expected, &got) {
                return Err(self.mismatch(&expected, t, a.line));
            }
        }
        Ok((
            CExpr::HostCall(function.host, vs),
            self.host_ty(&function.ret),
        ))
    }

    /// Appel d'une fonction du joueur (ou d'une méthode, avec son receveur).
    fn call_user(
        &mut self,
        sig: &FnSig,
        receiver: Option<&Expr>,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        if sig.params.len() != args.len() {
            return Err(self.arg_count(&sig.name, sig.params.len(), args.len(), line));
        }
        let mut parts = Vec::new();
        let mut writebacks: Vec<Place> = Vec::new();
        let mut uses: Vec<(usize, Vec<u32>, Use, u32)> = Vec::new();
        let mut all: Vec<(Option<&Expr>, Ty, Option<SelfKind>)> = Vec::new();
        if let (Some(kind), Some(ty)) = (sig.self_kind, &sig.self_ty) {
            all.push((receiver, ty.clone(), Some(kind)));
        }
        for (a, p) in args.iter().zip(&sig.params) {
            all.push((Some(a), p.clone(), None));
        }
        for (arg, param, self_kind) in all {
            let arg = arg.expect("argument");
            let mut pre = Vec::new();
            match (self.types.shallow(&param), self_kind) {
                (Ty::Ref(true, inner), _) => {
                    // `&mut x` (ou le receveur d'une méthode &mut self) : emplacement modifiable.
                    let target = match (&arg.kind, self_kind) {
                        (ExprKind::Ref(true, inner_expr), None) => inner_expr.as_ref(),
                        (_, Some(_)) => arg,
                        _ if self.is_place_expr(arg) => arg,
                        _ => {
                            let (_, t) = self.expr(arg, &mut pre)?;
                            return Err(self.mismatch(&param, &t, arg.line));
                        }
                    };
                    match self.place(target, &mut pre)? {
                        Some(mut place) => {
                            self.check_use(&place, arg.line)?;
                            let explicit =
                                matches!(&arg.kind, ExprKind::Ref(true, _)) || self_kind.is_some();
                            self.auto_deref(&mut place);
                            if !explicit && place.through.is_none() {
                                return Err(self.mismatch(&param, &place.ty, arg.line));
                            }
                            self.check_mutable(&place, arg.line, true)?;
                            if !self.types.unify(&inner, &place.ty) {
                                let got = Ty::Ref(true, Box::new(place.ty.clone()));
                                return Err(self.mismatch(&param, &got, arg.line));
                            }
                            uses.push((place.local, place.move_path.clone(), Use::Mut, arg.line));
                            parts.push((pre, self.place_expr(&place)));
                            writebacks.push(place);
                        }
                        None => {
                            // Valeur temporaire : la modification est perdue (comme en Rust).
                            let inner_expr = match &arg.kind {
                                ExprKind::Ref(true, e) => e.as_ref(),
                                _ => arg,
                            };
                            let (v, t) = self.value(inner_expr, &mut pre)?;
                            let t = self.types.strip(&t);
                            self.expect(&inner, &t, arg.line)?;
                            parts.push((pre, v));
                        }
                    }
                }
                (Ty::Ref(false, inner), _) => {
                    let (v, t) = self.expr(arg, &mut pre)?;
                    let got = if self_kind.is_some() {
                        self.types.strip(&t)
                    } else {
                        match self.types.shallow(&t) {
                            Ty::Ref(_, x) => *x,
                            _ => return Err(self.mismatch(&param, &t, arg.line)),
                        }
                    };
                    let got = if self_kind.is_some() {
                        got
                    } else {
                        self.types.shallow(&got)
                    };
                    if !self.types.unify(&inner, &got) {
                        // `&String` passe là où `&str` est attendu.
                        let text = matches!(self.types.shallow(&inner), Ty::Str)
                            && matches!(self.types.strip(&got), Ty::String);
                        if !text {
                            return Err(self.mismatch(&param, &t, arg.line));
                        }
                    }
                    let borrowed = match &arg.kind {
                        ExprKind::Ref(false, e) => Some(e.as_ref()),
                        _ if self_kind.is_some() => Some(arg),
                        _ => None,
                    };
                    if let Some(b) = borrowed
                        && self.is_place_expr(b)
                    {
                        let mut scratch = Vec::new();
                        let place = self.place(b, &mut scratch)?.unwrap();
                        uses.push((place.local, place.move_path.clone(), Use::Shared, arg.line));
                    }
                    parts.push((pre, v));
                }
                (_, Some(SelfKind::Value)) => {
                    let (v, t) = if self.is_place_expr(arg) {
                        let mut place = self.place(arg, &mut pre)?.unwrap();
                        self.check_use(&place, arg.line)?;
                        self.auto_deref(&mut place);
                        self.do_move(&place, arg.line)?;
                        uses.push((place.local, place.move_path.clone(), Use::Move, arg.line));
                        (self.place_expr(&place), place.ty.clone())
                    } else {
                        let (v, t) = self.value(arg, &mut pre)?;
                        (v, self.types.strip(&t))
                    };
                    self.expect(&param, &t, arg.line)?;
                    parts.push((pre, v));
                }
                _ => {
                    if self.is_place_expr(arg) {
                        let mut scratch = Vec::new();
                        let place = self.place(arg, &mut scratch)?.unwrap();
                        if !self.types.is_copy(&place.ty) {
                            uses.push((place.local, place.move_path.clone(), Use::Move, arg.line));
                        }
                    }
                    let (v, t) = self.value(arg, &mut pre)?;
                    self.expect(&param, &t, arg.line)?;
                    parts.push((pre, v));
                }
            }
        }
        // Emprunts en conflit dans un même appel.
        for (i, a) in uses.iter().enumerate() {
            for b in &uses[i + 1..] {
                let overlap = a.0 == b.0 && (is_prefix(&a.1, &b.1) || is_prefix(&b.1, &a.1));
                if !overlap || (a.2 == Use::Shared && b.2 == Use::Shared) {
                    continue;
                }
                let var = self.f.locals[a.0].name.clone();
                let (code, fr, en) = match (a.2, b.2) {
                    (Use::Mut, Use::Mut) => (
                        "E0499",
                        format!(
                            "« {var} » est empruntée deux fois en écriture (&mut) en même temps : une seule référence &mut à la fois"
                        ),
                        format!(
                            "'{var}' is borrowed as mutable (&mut) twice at the same time: only one &mut reference at a time"
                        ),
                    ),
                    (Use::Move, _) | (_, Use::Move) => (
                        "E0505",
                        format!(
                            "« {var} » est donnée (déplacée) alors qu'elle est aussi empruntée dans le même appel"
                        ),
                        format!(
                            "'{var}' is given away (moved) while also borrowed in the same call"
                        ),
                    ),
                    _ => (
                        "E0502",
                        format!(
                            "« {var} » est empruntée en écriture (&mut) et en lecture (&) en même temps : c'est l'un ou l'autre"
                        ),
                        format!(
                            "'{var}' is borrowed as mutable (&mut) and as shared (&) at the same time: it's one or the other"
                        ),
                    ),
                };
                return Err(rerr(b.3.max(a.3), code, fr, en));
            }
        }
        let vs = self.sequence(parts, line, out);
        let call = CExpr::Call(Box::new(name(&sig.slot)), vs);
        if !sig.has_mut {
            return Ok((call, sig.ret.clone()));
        }
        let r = self.temp();
        out.push(CStmt {
            line,
            kind: K::Assign(Target::Name(r.clone()), call),
        });
        for (j, place) in writebacks.iter().enumerate() {
            self.store(place, index(name(&r), int(j as i64 + 1)), line, out);
        }
        Ok((index(name(&r), int(0)), sig.ret.clone()))
    }

    // ----- Méthodes -----

    fn method(
        &mut self,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        // Intervalles : (a..b).rev(), (a..b).step_by(k).
        if let ExprKind::Range(Some(a), Some(b), inclusive) = &receiver.kind
            && (method == "rev" || method == "step_by")
        {
            let mut items = vec![(**a).clone(), (**b).clone()];
            items.extend(args.iter().cloned());
            let (vs, tys) = self.values(&items, line, out)?;
            for t in &tys {
                self.expect_int(t, line, method)?;
            }
            let start = vs[0].clone();
            let stop = if *inclusive {
                bin(CBin::Add, vs[1].clone(), int(1))
            } else {
                vs[1].clone()
            };
            let range = if method == "rev" {
                let stop = self.spill(stop, line, out);
                let start = self.spill(start, line, out);
                vec![
                    bin(CBin::Sub, stop, int(1)),
                    bin(CBin::Sub, start, int(1)),
                    int(-1),
                ]
            } else {
                if args.len() != 1 {
                    return Err(self.arg_count("step_by", 1, args.len(), line));
                }
                vec![start, stop, vs[2].clone()]
            };
            return Ok((CExpr::BuiltinCall(Builtin::Range, range), Ty::Range));
        }
        let place = self.place(receiver, out)?;
        let (rty, value) = match &place {
            Some(p) => (p.ty.clone(), None),
            None => {
                let (v, t) = self.value(receiver, out)?;
                (t, Some(v))
            }
        };
        let base = self.types.strip(&rty);
        let kind = Recv::new(self, &base, method, receiver.line)?;
        // Méthodes du joueur.
        if let Recv::User(sig) = &kind {
            if let Some(v) = value {
                // Receveur temporaire : rangé dans une variable cachée.
                let t = self.temp();
                out.push(CStmt {
                    line,
                    kind: K::Assign(Target::Name(t.clone()), v),
                });
                let slot_local = self.declare(&t, rty.clone(), true);
                let _ = slot_local;
                let fake = Expr {
                    kind: ExprKind::Path(vec![t]),
                    line: receiver.line,
                };
                let result = self.call_user(sig, Some(&fake), args, line, out);
                return result;
            }
            return self.call_user(sig, Some(receiver), args, line, out);
        }
        // Lecture, modification ou consommation du receveur.
        let recv_expr = match (&place, value) {
            (Some(p), _) => {
                let mut p = p.clone();
                self.check_use(&p, receiver.line)?;
                self.auto_deref(&mut p);
                match kind.mode() {
                    Mode::Read => {}
                    Mode::Mut => self.check_mutable(&p, receiver.line, true)?,
                    Mode::Consume => self.do_move(&p, receiver.line)?,
                }
                self.place_expr(&p)
            }
            (None, Some(v)) => v,
            (None, None) => unreachable!(),
        };
        let mut recv_place = place.clone();
        if let Some(p) = &mut recv_place {
            self.auto_deref(p);
        }
        let (arg_vs, arg_tys) = {
            let mut parts = vec![(Vec::new(), recv_expr)];
            let mut tys = Vec::new();
            for a in args {
                let mut pre = Vec::new();
                let (v, t) = if kind.borrows_args() {
                    self.expr(a, &mut pre)?
                } else {
                    self.value(a, &mut pre)?
                };
                parts.push((pre, v));
                tys.push(t);
            }
            (self.sequence(parts, line, out), tys)
        };
        let recv = arg_vs[0].clone();
        let args_v = &arg_vs[1..];
        let expected_args = kind.arg_count();
        if args.len() != expected_args {
            return Err(self.arg_count(&format!(".{method}()"), expected_args, args.len(), line));
        }
        self.builtin_method(
            kind,
            recv,
            recv_place.as_ref(),
            args_v,
            &arg_tys,
            args,
            line,
            out,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn builtin_method(
        &mut self,
        kind: Recv,
        recv: CExpr,
        place: Option<&Place>,
        args: &[CExpr],
        arg_tys: &[Ty],
        arg_exprs: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let stmt = |kind| CStmt { line, kind };
        let arg = |i: usize| args[i].clone();
        let store = |l: &mut Self, value: CExpr, out: &mut Vec<CStmt>| {
            if let Some(p) = place {
                l.store(p, value, line, out);
            }
        };
        Ok(match kind {
            Recv::User(_) => unreachable!(),
            Recv::Clone(ty) => (recv, ty),
            Recv::Getter(i, ty) => (index(recv, int(i as i64)), ty),
            Recv::IntAbs => (CExpr::BuiltinCall(Builtin::Abs, vec![recv]), Ty::Int),
            Recv::IntPow => {
                self.expect_int(&arg_tys[0], line, "pow")?;
                (bin(CBin::Pow, recv, arg(0)), Ty::Int)
            }
            Recv::IntMinMax(max) => {
                self.expect_int(&arg_tys[0], line, if max { "max" } else { "min" })?;
                (
                    CExpr::BuiltinCall(
                        if max { Builtin::Max } else { Builtin::Min },
                        vec![recv, arg(0)],
                    ),
                    Ty::Int,
                )
            }
            Recv::IntClamp => {
                self.expect_int(&arg_tys[0], line, "clamp")?;
                self.expect_int(&arg_tys[1], line, "clamp")?;
                let low = CExpr::BuiltinCall(Builtin::Min, vec![recv, arg(1)]);
                (CExpr::BuiltinCall(Builtin::Max, vec![low, arg(0)]), Ty::Int)
            }
            Recv::IntAbsDiff => {
                self.expect_int(&arg_tys[0], line, "abs_diff")?;
                (
                    CExpr::BuiltinCall(Builtin::Abs, vec![bin(CBin::Sub, recv, arg(0))]),
                    Ty::Int,
                )
            }
            Recv::IntRemEuclid => {
                self.expect_int(&arg_tys[0], line, "rem_euclid")?;
                (bin(CBin::Mod, recv, arg(0)), Ty::Int)
            }
            Recv::IntSign(positive) => (
                bin(if positive { CBin::Gt } else { CBin::Lt }, recv, int(0)),
                Ty::Bool,
            ),
            Recv::ToString(ty) => {
                let desc = CExpr::Const(self.types.descriptor(&ty));
                (
                    CExpr::BuiltinCall(Builtin::Format, vec![recv, desc, CExpr::Bool(false)]),
                    Ty::String,
                )
            }
            Recv::Len => (CExpr::BuiltinCall(Builtin::Len, vec![recv]), Ty::Int),
            Recv::IsEmpty => (
                bin(
                    CBin::Eq,
                    CExpr::BuiltinCall(Builtin::Len, vec![recv]),
                    int(0),
                ),
                Ty::Bool,
            ),
            Recv::AsStr => (recv, Ty::Str),
            Recv::Chars => (recv, Ty::Iter(Box::new(Ty::Str))),
            Recv::StrContains => {
                if !matches!(self.types.strip(&arg_tys[0]), Ty::Str | Ty::String) {
                    return Err(self.mismatch(&Ty::Str, &arg_tys[0], line));
                }
                (
                    CExpr::BuiltinCall(Builtin::Contains, vec![recv, arg(0)]),
                    Ty::Bool,
                )
            }
            Recv::PushStr => {
                self.string_operand(&arg_tys[0], line)?;
                store(self, bin(CBin::Add, recv, arg(0)), out);
                (unit(), Ty::Unit)
            }
            Recv::VecPush(elem) => {
                self.expect(&elem, &arg_tys[0], arg_exprs[0].line)?;
                match place {
                    Some(p) if p.elems.is_empty() => {
                        let slot = self.f.locals[p.local].slot.clone();
                        out.push(stmt(K::Expr(CExpr::Method(
                            slot,
                            "append".into(),
                            vec![arg(0)],
                        ))));
                    }
                    _ => {
                        let t = self.temp();
                        out.push(stmt(K::Assign(Target::Name(t.clone()), recv)));
                        out.push(stmt(K::Expr(CExpr::Method(
                            t.clone(),
                            "append".into(),
                            vec![arg(0)],
                        ))));
                        store(self, name(&t), out);
                    }
                }
                (unit(), Ty::Unit)
            }
            Recv::VecPop(elem) => {
                let t = self.temp();
                out.push(stmt(K::Assign(
                    Target::Name(t.clone()),
                    CExpr::BuiltinCall(Builtin::Pop, vec![recv]),
                )));
                store(self, index(name(&t), int(0)), out);
                (index(name(&t), int(1)), Ty::Option(Box::new(elem)))
            }
            Recv::VecRemove(elem) => {
                self.expect_int(&arg_tys[0], line, "remove")?;
                let t = self.temp();
                out.push(stmt(K::Assign(
                    Target::Name(t.clone()),
                    CExpr::BuiltinCall(Builtin::Remove, vec![recv, arg(0)]),
                )));
                store(self, index(name(&t), int(0)), out);
                (index(name(&t), int(1)), elem)
            }
            Recv::VecInsert(elem) => {
                self.expect_int(&arg_tys[0], line, "insert")?;
                self.expect(&elem, &arg_tys[1], line)?;
                store(
                    self,
                    CExpr::BuiltinCall(Builtin::Insert, vec![recv, arg(0), arg(1)]),
                    out,
                );
                (unit(), Ty::Unit)
            }
            Recv::VecClear => {
                store(self, CExpr::List(Vec::new()), out);
                (unit(), Ty::Unit)
            }
            Recv::VecSort(elem) => {
                if !matches!(
                    self.types.strip(&elem),
                    Ty::Int | Ty::Str | Ty::String | Ty::Tuple(_) | Ty::Var(_)
                ) {
                    return Err(rerr(
                        line,
                        "E0277",
                        format!("on ne sait pas trier des « {} »", self.types.display(&elem)),
                        format!("'{}' values cannot be sorted", self.types.display(&elem)),
                    ));
                }
                store(self, CExpr::BuiltinCall(Builtin::Sort, vec![recv]), out);
                (unit(), Ty::Unit)
            }
            Recv::VecReverse => {
                store(self, CExpr::BuiltinCall(Builtin::Reverse, vec![recv]), out);
                (unit(), Ty::Unit)
            }
            Recv::VecSwap => {
                self.expect_int(&arg_tys[0], line, "swap")?;
                self.expect_int(&arg_tys[1], line, "swap")?;
                if let Some(p) = place {
                    let (i, j) = (self.spill(arg(0), line, out), self.spill(arg(1), line, out));
                    let a = self.temp();
                    out.push(stmt(K::Assign(
                        Target::Name(a.clone()),
                        index(recv.clone(), i.clone()),
                    )));
                    let mut pi = p.clone();
                    pi.elems.push(Elem::Index(i));
                    let mut pj = p.clone();
                    pj.elems.push(Elem::Index(j.clone()));
                    self.store(&pi, index(recv, j), line, out);
                    self.store(&pj, name(&a), line, out);
                }
                (unit(), Ty::Unit)
            }
            Recv::VecContains(elem) => {
                let got = self.types.strip(&arg_tys[0]);
                let elem_stripped = self.types.strip(&elem);
                if !matches!(self.types.shallow(&arg_tys[0]), Ty::Ref(..)) {
                    return Err(self.mismatch(&Ty::Ref(false, Box::new(elem)), &arg_tys[0], line));
                }
                self.expect(&elem_stripped, &got, line)?;
                (
                    CExpr::BuiltinCall(Builtin::Contains, vec![recv, arg(0)]),
                    Ty::Bool,
                )
            }
            Recv::VecEnd(elem, last) => {
                let r = self.spill(recv, line, out);
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), CExpr::None)));
                let length = CExpr::BuiltinCall(Builtin::Len, vec![r.clone()]);
                out.push(stmt(K::If(
                    vec![(
                        bin(CBin::Gt, length, int(0)),
                        vec![stmt(K::Assign(
                            Target::Name(t.clone()),
                            index(r, int(if last { -1 } else { 0 })),
                        ))],
                    )],
                    Vec::new(),
                )));
                (
                    name(&t),
                    Ty::Option(Box::new(Ty::Ref(false, Box::new(elem)))),
                )
            }
            Recv::VecGet(elem) => {
                self.expect_int(&arg_tys[0], line, "get")?;
                let r = self.spill(recv, line, out);
                let i = self.spill(arg(0), line, out);
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), CExpr::None)));
                let inside = CExpr::And(
                    Box::new(bin(CBin::Ge, i.clone(), int(0))),
                    Box::new(bin(
                        CBin::Lt,
                        i.clone(),
                        CExpr::BuiltinCall(Builtin::Len, vec![r.clone()]),
                    )),
                );
                out.push(stmt(K::If(
                    vec![(
                        inside,
                        vec![stmt(K::Assign(Target::Name(t.clone()), index(r, i)))],
                    )],
                    Vec::new(),
                )));
                (
                    name(&t),
                    Ty::Option(Box::new(Ty::Ref(false, Box::new(elem)))),
                )
            }
            Recv::Iter(item) => (recv, Ty::Iter(Box::new(item))),
            Recv::OptionIs(some) => (
                bin(if some { CBin::Ne } else { CBin::Eq }, recv, CExpr::None),
                Ty::Bool,
            ),
            Recv::OptionUnwrap(inner, message) => {
                let t = self.spill(recv, line, out);
                let failure = match message {
                    true => stmt(K::Panic(PanicKind::Message, Some(arg(0)))),
                    false => stmt(K::Panic(PanicKind::UnwrapNone, None)),
                };
                out.push(stmt(K::If(
                    vec![(bin(CBin::Eq, t.clone(), CExpr::None), vec![failure])],
                    Vec::new(),
                )));
                (t, inner)
            }
            Recv::OptionUnwrapOr(inner) => {
                self.expect(&inner, &arg_tys[0], line)?;
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), recv)));
                out.push(stmt(K::If(
                    vec![(
                        bin(CBin::Eq, name(&t), CExpr::None),
                        vec![stmt(K::Assign(Target::Name(t.clone()), arg(0)))],
                    )],
                    Vec::new(),
                )));
                (name(&t), inner)
            }
            Recv::OptionTake(inner) => {
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), recv)));
                store(self, CExpr::None, out);
                (name(&t), Ty::Option(Box::new(inner)))
            }
            Recv::OptionCopied(inner) => (recv, Ty::Option(Box::new(inner))),
            Recv::ResultIs(ok) => {
                let flag = index(recv, int(0));
                (if ok { flag } else { not(flag) }, Ty::Bool)
            }
            Recv::ResultUnwrap(inner, message) => {
                let t = self.spill(recv, line, out);
                let failure = match message {
                    true => stmt(K::Panic(PanicKind::Message, Some(arg(0)))),
                    false => stmt(K::Panic(PanicKind::UnwrapErr, None)),
                };
                out.push(stmt(K::If(
                    vec![(not(index(t.clone(), int(0))), vec![failure])],
                    Vec::new(),
                )));
                (index(t, int(1)), inner)
            }
            Recv::ResultUnwrapOr(inner) => {
                self.expect(&inner, &arg_tys[0], line)?;
                let r = self.spill(recv, line, out);
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), arg(0))));
                out.push(stmt(K::If(
                    vec![(
                        index(r.clone(), int(0)),
                        vec![stmt(K::Assign(Target::Name(t.clone()), index(r, int(1))))],
                    )],
                    Vec::new(),
                )));
                (name(&t), inner)
            }
            Recv::ResultSide(inner, ok) => {
                let r = self.spill(recv, line, out);
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), CExpr::None)));
                let flag = index(r.clone(), int(0));
                let condition = if ok { flag } else { not(flag) };
                out.push(stmt(K::If(
                    vec![(
                        condition,
                        vec![stmt(K::Assign(Target::Name(t.clone()), index(r, int(1))))],
                    )],
                    Vec::new(),
                )));
                (name(&t), Ty::Option(Box::new(inner)))
            }
            Recv::IterSum(item) => {
                self.expect_int(&item, line, "sum")?;
                (CExpr::BuiltinCall(Builtin::Sum, vec![recv]), Ty::Int)
            }
            Recv::IterCount => (CExpr::BuiltinCall(Builtin::Len, vec![recv]), Ty::Int),
            Recv::IterMinMax(item, max) => {
                let stripped = self.types.strip(&item);
                self.expect_int(&stripped, line, if max { "max" } else { "min" })?;
                let r = self.spill(recv, line, out);
                let t = self.temp();
                out.push(stmt(K::Assign(Target::Name(t.clone()), CExpr::None)));
                let length = CExpr::BuiltinCall(Builtin::Len, vec![r.clone()]);
                let extreme =
                    CExpr::BuiltinCall(if max { Builtin::Max } else { Builtin::Min }, vec![r]);
                out.push(stmt(K::If(
                    vec![(
                        bin(CBin::Gt, length, int(0)),
                        vec![stmt(K::Assign(Target::Name(t.clone()), extreme))],
                    )],
                    Vec::new(),
                )));
                (name(&t), Ty::Option(Box::new(item)))
            }
            Recv::IterRev(item) => (
                CExpr::BuiltinCall(Builtin::Reverse, vec![recv]),
                Ty::Iter(Box::new(item)),
            ),
            Recv::IterEnumerate(item) => (
                CExpr::BuiltinCall(Builtin::Enumerate, vec![recv]),
                Ty::Iter(Box::new(Ty::Tuple(vec![Ty::Int, item]))),
            ),
            Recv::IterCollect(item) => {
                let item = self.types.strip(&item);
                (recv, Ty::Vec(Box::new(item)))
            }
            Recv::IterCopied(item) => {
                let item = self.types.strip(&item);
                (recv, Ty::Iter(Box::new(item)))
            }
            Recv::RangeContains => {
                self.expect_int(&arg_tys[0], line, "contains")?;
                (
                    CExpr::BuiltinCall(Builtin::Contains, vec![recv, arg(0)]),
                    Ty::Bool,
                )
            }
        })
    }

    // ----- Conditions et boucles -----

    fn if_expr(
        &mut self,
        condition: &Expr,
        then: &Block,
        otherwise: Option<&Expr>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let (c, t) = self.expr(condition, out)?;
        self.expect_bool(&t, condition.line)?;
        let entry = self.f.flow.clone();
        let mut then_out = Vec::new();
        let (then_v, then_t) = self.block(then, &mut then_out)?;
        let then_flow = mem::replace(&mut self.f.flow, entry);
        let mut else_out = Vec::new();
        let (else_v, else_t) = match otherwise {
            Some(e) => self.value(e, &mut else_out)?,
            None => (unit(), Ty::Unit),
        };
        let else_flow = mem::replace(&mut self.f.flow, Flow::new());
        self.f.flow = merge(then_flow, else_flow);
        if otherwise.is_none()
            && !matches!(self.types.shallow(&then_t), Ty::Unit | Ty::Never)
            && !self.types.unify(&then_t, &Ty::Unit)
        {
            return Err(rerr(
                then.end_line,
                "E0317",
                format!(
                    "un if sans else ne peut pas valoir « {} » : ajoute une branche else, ou un « ; »",
                    self.types.display(&then_t)
                ),
                format!(
                    "an if without else cannot be '{}': add an else branch, or a ';'",
                    self.types.display(&then_t)
                ),
            ));
        }
        let result = self.types.fresh();
        if !self.types.unify(&result, &then_t) || !self.types.unify(&result, &else_t) {
            return Err(rerr(
                line,
                "E0308",
                format!(
                    "les deux branches du if n'ont pas le même type : « {} » et « {} »",
                    self.types.display(&then_t),
                    self.types.display(&else_t)
                ),
                format!(
                    "the two branches of the if have different types: '{}' and '{}'",
                    self.types.display(&then_t),
                    self.types.display(&else_t)
                ),
            ));
        }
        let (value, ty) = self.branch_values(
            vec![
                (&mut then_out, then_v, &then_t),
                (&mut else_out, else_v, &else_t),
            ],
            &result,
            line,
        );
        out.push(CStmt {
            line,
            kind: K::If(vec![(c, then_out)], else_out),
        });
        Ok((value, ty))
    }

    /// Si les branches ont une valeur, chacune la range dans une même variable.
    fn branch_values(
        &mut self,
        branches: Vec<(&mut Vec<CStmt>, CExpr, &Ty)>,
        result: &Ty,
        line: u32,
    ) -> (CExpr, Ty) {
        let resolved = self.types.shallow(result);
        if matches!(resolved, Ty::Unit | Ty::Never) {
            for (stmts, v, _) in branches {
                if !is_simple(&v) {
                    stmts.push(CStmt {
                        line,
                        kind: K::Expr(v),
                    });
                }
            }
            return (unit(), resolved);
        }
        let t = self.temp();
        for (stmts, v, ty) in branches {
            if !matches!(self.types.shallow(ty), Ty::Never) {
                stmts.push(CStmt {
                    line,
                    kind: K::Assign(Target::Name(t.clone()), v),
                });
            }
        }
        (name(&t), resolved)
    }

    /// Valeur sur laquelle porte un `match` / `if let` : une expression sans effet.
    fn scrutinee(&mut self, e: &Expr, out: &mut Vec<CStmt>) -> R<(CExpr, Ty, Option<Place>)> {
        if let Some(place) = self.place(e, out)? {
            self.check_use(&place, e.line)?;
            let read = self.place_expr(&place);
            let read = self.spill(read, e.line, out);
            let ty = place.ty.clone();
            return Ok((read, ty, Some(place)));
        }
        let (v, t) = self.expr_inner(e, out)?;
        let v = self.spill(v, e.line, out);
        Ok((v, t, None))
    }

    fn if_let(
        &mut self,
        pat: &Pat,
        scrutinee: &Expr,
        then: &Block,
        otherwise: Option<&Expr>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let (s, sty, splace) = self.scrutinee(scrutinee, out)?;
        let entry = self.f.flow.clone();
        self.push_scope();
        let mut binds = Vec::new();
        let test = self.pattern(pat, &s, &sty, &mut binds, None, false, line)?;
        let mut then_out = Vec::new();
        self.bind(binds, splace.as_ref(), line, &mut then_out)?;
        let (then_v, then_t) = self.block(then, &mut then_out)?;
        self.pop_scope();
        let then_flow = mem::replace(&mut self.f.flow, entry);
        let mut else_out = Vec::new();
        let (else_v, else_t) = match otherwise {
            Some(e) => self.value(e, &mut else_out)?,
            None => (unit(), Ty::Unit),
        };
        let else_flow = mem::replace(&mut self.f.flow, Flow::new());
        self.f.flow = merge(then_flow, else_flow);
        let result = self.types.fresh();
        if !self.types.unify(&result, &then_t) || !self.types.unify(&result, &else_t) {
            return Err(rerr(
                line,
                "E0308",
                format!(
                    "les deux branches n'ont pas le même type : « {} » et « {} »",
                    self.types.display(&then_t),
                    self.types.display(&else_t)
                ),
                format!(
                    "the two branches have different types: '{}' and '{}'",
                    self.types.display(&then_t),
                    self.types.display(&else_t)
                ),
            ));
        }
        let (value, ty) = self.branch_values(
            vec![
                (&mut then_out, then_v, &then_t),
                (&mut else_out, else_v, &else_t),
            ],
            &result,
            line,
        );
        match test {
            Some(test) => out.push(CStmt {
                line,
                kind: K::If(vec![(test, then_out)], else_out),
            }),
            None => out.extend(then_out),
        }
        Ok((value, ty))
    }

    fn match_expr(
        &mut self,
        scrutinee: &Expr,
        arms: &[Arm],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let (s, sty, splace) = self.scrutinee(scrutinee, out)?;
        let entry = self.f.flow.clone();
        let result = self.types.fresh();
        let mut flows = Vec::new();
        let mut lowered = Vec::new();
        for arm in arms {
            self.f.flow = entry.clone();
            self.push_scope();
            let mut binds = Vec::new();
            let test = self.pattern(&arm.pat, &s, &sty, &mut binds, None, false, arm.line)?;
            let mut binds_out = Vec::new();
            self.bind(binds, splace.as_ref(), arm.line, &mut binds_out)?;
            let guard = match &arm.guard {
                Some(g) => {
                    let mut pre = Vec::new();
                    let (gv, gt) = self.expr(g, &mut pre)?;
                    self.expect_bool(&gt, g.line)?;
                    Some((pre, gv))
                }
                None => None,
            };
            let mut body = Vec::new();
            let (bv, bt) = self.value(&arm.body, &mut body)?;
            if !self.types.unify(&result, &bt) {
                return Err(rerr(
                    arm.line,
                    "E0308",
                    format!(
                        "les branches du match n'ont pas le même type : « {} » et « {} »",
                        self.types.display(&result),
                        self.types.display(&bt)
                    ),
                    format!(
                        "the match arms have different types: '{}' and '{}'",
                        self.types.display(&result),
                        self.types.display(&bt)
                    ),
                ));
            }
            self.pop_scope();
            flows.push(mem::replace(&mut self.f.flow, Flow::new()));
            lowered.push((test, binds_out, guard, body, bv, bt));
        }
        self.f.flow = flows.into_iter().fold(Flow::unreachable(), merge);
        let rows: Vec<Vec<&Pat>> = arms
            .iter()
            .filter(|a| a.guard.is_none())
            .map(|a| vec![&a.pat])
            .collect();
        if !self.exhaustive(rows, std::slice::from_ref(&sty)) {
            return Err(rerr(
                line,
                "E0004",
                "le match ne couvre pas tous les cas possibles : ajoute les cas manquants, ou une branche _ => …",
                "the match does not cover every possible case: add the missing cases, or a _ => … arm",
            ));
        }
        // Valeurs des branches.
        let mut bodies: Vec<Vec<CStmt>> = Vec::new();
        let mut values = Vec::new();
        let mut parts = Vec::new();
        for (test, binds_out, guard, body, bv, bt) in lowered {
            bodies.push(body);
            values.push((bv, bt));
            parts.push((test, binds_out, guard));
        }
        let (value, ty) = {
            let branches: Vec<(&mut Vec<CStmt>, CExpr, &Ty)> = bodies
                .iter_mut()
                .zip(&values)
                .map(|(b, (v, t))| (b, v.clone(), t))
                .collect();
            self.branch_values(branches, &result, line)
        };
        let has_guards = parts.iter().any(|(_, _, g)| g.is_some());
        if !has_guards {
            let mut otherwise: Vec<CStmt> = Vec::new();
            for ((test, binds_out, _), body) in parts.into_iter().zip(bodies).rev() {
                let mut code = binds_out;
                code.extend(body);
                otherwise = match test {
                    None => code,
                    Some(test) => vec![CStmt {
                        line,
                        kind: K::If(vec![(test, code)], otherwise),
                    }],
                };
            }
            out.extend(otherwise);
        } else {
            let done = self.temp();
            out.push(CStmt {
                line,
                kind: K::Assign(Target::Name(done.clone()), CExpr::Bool(false)),
            });
            for ((test, binds_out, guard), body) in parts.into_iter().zip(bodies) {
                let mut taken = vec![CStmt {
                    line,
                    kind: K::Assign(Target::Name(done.clone()), CExpr::Bool(true)),
                }];
                taken.extend(body);
                let mut code = binds_out;
                match guard {
                    Some((pre, g)) => {
                        code.extend(pre);
                        code.push(CStmt {
                            line,
                            kind: K::If(vec![(g, taken)], Vec::new()),
                        });
                    }
                    None => code.extend(taken),
                }
                let condition = match test {
                    Some(test) => CExpr::And(Box::new(not(name(&done))), Box::new(test)),
                    None => not(name(&done)),
                };
                out.push(CStmt {
                    line,
                    kind: K::If(vec![(condition, code)], Vec::new()),
                });
            }
        }
        Ok((value, ty))
    }

    fn begin_loop(&mut self, is_loop: bool) -> Flow {
        let entry = self.f.flow.clone();
        self.f.loops.push(LoopCtx {
            boundary: self.f.locals.len(),
            breaks: Vec::new(),
            continues: Vec::new(),
            touched: HashMap::new(),
            is_loop,
            value: None,
            writeback: Vec::new(),
        });
        entry
    }

    /// Fin de boucle : vérifie qu'aucune valeur extérieure n'est déplacée à chaque tour.
    fn end_loop(&mut self, entry: Flow) -> R<LoopCtx> {
        let ctx = self.f.loops.pop().unwrap();
        let back = ctx
            .continues
            .iter()
            .cloned()
            .fold(self.f.flow.clone(), merge);
        if back.reachable {
            for m in &back.moved {
                let before = entry
                    .moved
                    .iter()
                    .any(|e| e.local == m.local && e.path == m.path);
                if m.local < ctx.boundary && !before && ctx.touched.get(&m.local) == Some(&true) {
                    let var = self.f.locals[m.local].name.clone();
                    return Err(rerr(
                        m.line,
                        "E0382",
                        format!(
                            "« {var} » est déplacée dans la boucle : au tour suivant, elle n'existe plus. Déplace-la une seule fois (avant break), donne-lui une nouvelle valeur, ou prête-la avec &"
                        ),
                        format!(
                            "'{var}' is moved inside the loop: on the next iteration it no longer exists. Move it only once (before break), give it a new value, or lend it with &"
                        ),
                    ));
                }
            }
        }
        let breaks = ctx.breaks.iter().cloned().fold(Flow::unreachable(), merge);
        self.f.flow = if ctx.is_loop {
            breaks
        } else {
            merge(merge(entry, back), breaks)
        };
        Ok(ctx)
    }

    fn loop_expr(
        &mut self,
        head: Option<LoopHead>,
        body: &Block,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let is_loop = head.is_none();
        let entry = self.begin_loop(is_loop);
        let mut code = Vec::new();
        let mut condition = CExpr::Bool(true);
        self.push_scope();
        match head {
            None => {}
            Some(LoopHead::While(c)) => {
                let mut pre = Vec::new();
                let (cv, ct) = self.expr(c, &mut pre)?;
                self.expect_bool(&ct, c.line)?;
                if pre.is_empty() {
                    condition = cv;
                } else {
                    code.extend(pre);
                    code.push(CStmt {
                        line,
                        kind: K::If(
                            vec![(
                                not(cv),
                                vec![CStmt {
                                    line,
                                    kind: K::Break,
                                }],
                            )],
                            Vec::new(),
                        ),
                    });
                }
            }
            Some(LoopHead::WhileLet(pat, scrutinee)) => {
                let (s, sty, splace) = self.scrutinee(scrutinee, &mut code)?;
                let mut binds = Vec::new();
                let test = self.pattern(pat, &s, &sty, &mut binds, None, false, line)?;
                if let Some(test) = test {
                    code.push(CStmt {
                        line,
                        kind: K::If(
                            vec![(
                                not(test),
                                vec![CStmt {
                                    line,
                                    kind: K::Break,
                                }],
                            )],
                            Vec::new(),
                        ),
                    });
                }
                self.bind(binds, splace.as_ref(), line, &mut code)?;
            }
        }
        let (bv, _) = self.block(body, &mut code)?;
        self.discard(bv, body.end_line, &mut code);
        self.pop_scope();
        let ctx = self.end_loop(entry)?;
        out.push(CStmt {
            line,
            kind: K::While(condition, code),
        });
        if !is_loop {
            return Ok((unit(), Ty::Unit));
        }
        Ok(match ctx.value {
            Some((slot, ty)) => (name(&slot), ty),
            None if ctx.breaks.is_empty() => (unit(), Ty::Never),
            None => (unit(), Ty::Unit),
        })
    }

    fn for_loop(
        &mut self,
        pat: &Pat,
        iterable: &Expr,
        body: &Block,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        // `for x in v.iter_mut()` ou `for x in &mut v` : chaque élément est modifié en place.
        let mutable_source = match &iterable.kind {
            ExprKind::Ref(true, inner) => Some(inner.as_ref()),
            ExprKind::MethodCall(recv, m, args) if m == "iter_mut" && args.is_empty() => {
                Some(recv.as_ref())
            }
            _ => None,
        };
        if let Some(source) = mutable_source {
            let Some(mut place) = self.place(source, out)? else {
                return Err(unsupported(
                    line,
                    "iter_mut() sur une valeur temporaire",
                    "iter_mut() on a temporary value",
                ));
            };
            self.check_use(&place, line)?;
            self.auto_deref(&mut place);
            self.check_mutable(&place, line, true)?;
            let elem = self.index_elem_ty(&place.ty, line)?;
            let Pat::Bind { name: var, .. } = pat else {
                return Err(unsupported(
                    line,
                    "un motif avec iter_mut() (utilise une simple variable)",
                    "a pattern with iter_mut() (use a plain variable)",
                ));
            };
            let k = self.temp();
            let list = self.place_expr(&place);
            let range = CExpr::BuiltinCall(
                Builtin::Range,
                vec![CExpr::BuiltinCall(Builtin::Len, vec![list])],
            );
            let entry = self.begin_loop(false);
            self.push_scope();
            let slot = self.declare(var, Ty::Ref(true, Box::new(elem)), false);
            let mut item = place.clone();
            item.elems.push(Elem::Index(name(&k)));
            let mut code = vec![CStmt {
                line,
                kind: K::Assign(Target::Name(slot.clone()), self.place_expr(&item)),
            }];
            let mut writeback = Vec::new();
            self.store(&item, name(&slot), line, &mut writeback);
            self.f.loops.last_mut().unwrap().writeback = writeback.clone();
            let (bv, _) = self.block(body, &mut code)?;
            self.discard(bv, body.end_line, &mut code);
            if self.f.flow.reachable {
                code.extend(writeback);
            }
            self.pop_scope();
            self.end_loop(entry)?;
            out.push(CStmt {
                line,
                kind: K::For(Target::Name(k), range, code),
            });
            return Ok((unit(), Ty::Unit));
        }
        let (iter_v, iter_t) = self.value(iterable, out)?;
        let item = match self.types.shallow(&iter_t) {
            Ty::Range => Ty::Int,
            Ty::Vec(t) | Ty::Iter(t) => *t,
            Ty::Ref(m, inner) => match self.types.shallow(&inner) {
                Ty::Vec(t) => Ty::Ref(m, t),
                Ty::Iter(t) => *t,
                other => return Err(self.not_iterable(&other, line)),
            },
            Ty::Var(_) => {
                return Err(rerr(
                    line,
                    "E0282",
                    "le type de ce que parcourt la boucle est encore inconnu : précise-le",
                    "the type of what the loop goes through is not known yet: specify it",
                ));
            }
            other => return Err(self.not_iterable(&other, line)),
        };
        let entry = self.begin_loop(false);
        self.push_scope();
        let mut code = Vec::new();
        let target = match pat {
            Pat::Bind {
                name: var,
                mutable,
                by_ref: false,
            } if !self.is_const_pattern(var) => Target::Name(self.declare(var, item, *mutable)),
            _ => {
                let t = self.temp();
                let mut binds = Vec::new();
                if self
                    .pattern(pat, &name(&t), &item, &mut binds, None, false, line)?
                    .is_some()
                {
                    return Err(rerr(
                        line,
                        "E0005",
                        "le motif d'une boucle for doit toujours correspondre",
                        "the pattern of a for loop must always match",
                    ));
                }
                self.bind(binds, None, line, &mut code)?;
                Target::Name(t)
            }
        };
        let (bv, _) = self.block(body, &mut code)?;
        self.discard(bv, body.end_line, &mut code);
        self.pop_scope();
        self.end_loop(entry)?;
        out.push(CStmt {
            line,
            kind: K::For(target, iter_v, code),
        });
        Ok((unit(), Ty::Unit))
    }

    fn not_iterable(&self, ty: &Ty, line: u32) -> ScriptError {
        let (fr, en) = match ty {
            Ty::Str | Ty::String => (" (utilise .chars())", " (use .chars())"),
            _ => ("", ""),
        };
        rerr(
            line,
            "E0277",
            format!(
                "« {} » ne se parcourt pas avec for{fr}",
                self.types.display(ty)
            ),
            format!(
                "'{}' cannot be looped over with for{en}",
                self.types.display(ty)
            ),
        )
    }

    fn try_operator(&mut self, inner: &Expr, line: u32, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        let (v, t) = self.value(inner, out)?;
        let ret = self.types.shallow(&self.f.ret.clone());
        let t = self.types.shallow(&t);
        let v = self.spill(v, line, out);
        match (&t, &ret) {
            (Ty::Option(inner_t), Ty::Option(_)) if !self.f.is_main => {
                let mut early = Vec::new();
                self.return_stmts(CExpr::None, line, &mut early);
                out.push(CStmt {
                    line,
                    kind: K::If(
                        vec![(bin(CBin::Eq, v.clone(), CExpr::None), early)],
                        Vec::new(),
                    ),
                });
                Ok((v, (**inner_t).clone()))
            }
            (Ty::Result(ok, err), Ty::Result(_, ret_err)) if !self.f.is_main => {
                if !self.types.unify(err, ret_err) {
                    return Err(self.mismatch(ret_err, err, line));
                }
                let mut early = Vec::new();
                self.return_stmts(v.clone(), line, &mut early);
                out.push(CStmt {
                    line,
                    kind: K::If(vec![(not(index(v.clone(), int(0))), early)], Vec::new()),
                });
                Ok((index(v, int(1)), (**ok).clone()))
            }
            _ => Err(rerr(
                line,
                "E0277",
                "« ? » s'utilise sur une Option ou un Result, dans une fonction qui renvoie le même genre (pas dans main)",
                "'?' is used on an Option or a Result, inside a function returning the same kind (not in main)",
            )),
        }
    }

    // ----- Affichage -----

    fn format_macro(
        &mut self,
        kind: FmtMacro,
        template: &str,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let pieces = parse_format(template, line)?;
        // Arguments utilisés : positionnels, puis variables nommées dans le texte.
        let mut exprs: Vec<Expr> = args.to_vec();
        let mut next = 0;
        let mut resolved = Vec::new();
        for piece in &pieces {
            match piece {
                Piece::Text(t) => resolved.push(Piece::Text(t.clone())),
                Piece::Arg { which, debug } => {
                    let i = match which {
                        Which::Next => {
                            next += 1;
                            next - 1
                        }
                        Which::Index(i) => *i,
                        Which::Name(n) => {
                            exprs.push(Expr {
                                kind: ExprKind::Path(vec![n.clone()]),
                                line,
                            });
                            exprs.len() - 1
                        }
                    };
                    if i >= exprs.len() {
                        return Err(rerr(
                            line,
                            "",
                            format!(
                                "le texte contient plus de {{}} que d'arguments ({} donnés)",
                                args.len()
                            ),
                            format!(
                                "the text has more {{}} than arguments ({} given)",
                                args.len()
                            ),
                        ));
                    }
                    resolved.push(Piece::Arg {
                        which: Which::Index(i),
                        debug: *debug,
                    });
                }
            }
        }
        if next < args.len()
            && !pieces.iter().any(|p| {
                matches!(
                    p,
                    Piece::Arg {
                        which: Which::Index(_),
                        ..
                    }
                )
            })
        {
            return Err(rerr(
                line,
                "",
                format!(
                    "{} argument(s) en trop : chaque valeur doit avoir son {{}} dans le texte",
                    args.len() - next
                ),
                format!(
                    "{} extra argument(s): every value needs its {{}} in the text",
                    args.len() - next
                ),
            ));
        }
        let mut parts = Vec::new();
        let mut tys = Vec::new();
        for e in &exprs {
            let mut pre = Vec::new();
            let (v, t) = self.expr(e, &mut pre)?;
            parts.push((pre, v));
            tys.push(t);
        }
        let vs = self.sequence(parts, line, out);
        let mut text: Option<CExpr> = None;
        let mut push = |e: CExpr| {
            text = Some(match text.take() {
                None => e,
                Some(previous) => bin(CBin::Add, previous, e),
            });
        };
        for piece in resolved {
            match piece {
                Piece::Text(t) => push(CExpr::Str(t)),
                Piece::Arg {
                    which: Which::Index(i),
                    debug,
                } => {
                    let ty = tys[i].clone();
                    let v = vs[i].clone();
                    let shown = self.types.display(&ty);
                    if debug {
                        if !self.types.has_debug(&ty) {
                            return Err(rerr(
                                exprs[i].line,
                                "E0277",
                                format!(
                                    "« {shown} » ne s'affiche pas avec {{:?}} : ajoute #[derive(Debug)] au-dessus de son type"
                                ),
                                format!(
                                    "'{shown}' cannot be shown with {{:?}}: add #[derive(Debug)] above its type"
                                ),
                            ));
                        }
                        let desc = CExpr::Const(self.types.descriptor(&ty));
                        push(CExpr::BuiltinCall(
                            Builtin::Format,
                            vec![v, desc, CExpr::Bool(true)],
                        ));
                    } else {
                        if !self.types.has_display(&ty) {
                            return Err(rerr(
                                exprs[i].line,
                                "E0277",
                                format!(
                                    "« {shown} » ne s'affiche pas avec {{}} : utilise {{:?}} (affichage de débogage)"
                                ),
                                format!(
                                    "'{shown}' cannot be shown with {{}}: use {{:?}} (debug display)"
                                ),
                            ));
                        }
                        match self.types.strip(&ty) {
                            Ty::Str | Ty::String => push(v),
                            Ty::Int => push(CExpr::BuiltinCall(Builtin::Str, vec![v])),
                            other => {
                                let desc = CExpr::Const(self.types.descriptor(&other));
                                push(CExpr::BuiltinCall(
                                    Builtin::Format,
                                    vec![v, desc, CExpr::Bool(false)],
                                ));
                            }
                        }
                    }
                }
                Piece::Arg { .. } => unreachable!(),
            }
        }
        let text = text.unwrap_or(CExpr::Str(String::new()));
        match kind {
            FmtMacro::Format => Ok((text, Ty::String)),
            FmtMacro::Println | FmtMacro::Print => {
                out.push(CStmt {
                    line,
                    kind: K::Expr(CExpr::HostCall(self.api.print, vec![text])),
                });
                Ok((unit(), Ty::Unit))
            }
            FmtMacro::Panic => {
                out.push(CStmt {
                    line,
                    kind: K::Panic(PanicKind::Message, Some(text)),
                });
                self.f.flow = Flow::unreachable();
                Ok((unit(), Ty::Never))
            }
        }
    }
}

enum LoopHead<'e> {
    While(&'e Expr),
    WhileLet(&'e Pat, &'e Expr),
}

enum Variant {
    User(String),
    Game(Value),
}

#[derive(Clone, Debug, PartialEq)]
enum Ctor {
    Bool(bool),
    Some,
    None,
    Ok,
    Err,
    Variant(String),
    Single,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Read,
    Mut,
    Consume,
}

/// Méthode reconnue, selon le type du receveur.
enum Recv {
    User(FnSig),
    Clone(Ty),
    Getter(u32, Ty),
    IntAbs,
    IntPow,
    IntMinMax(bool),
    IntClamp,
    IntAbsDiff,
    IntRemEuclid,
    IntSign(bool),
    ToString(Ty),
    Len,
    IsEmpty,
    AsStr,
    Chars,
    StrContains,
    PushStr,
    VecPush(Ty),
    VecPop(Ty),
    VecRemove(Ty),
    VecInsert(Ty),
    VecClear,
    VecSort(Ty),
    VecReverse,
    VecSwap,
    VecContains(Ty),
    VecEnd(Ty, bool),
    VecGet(Ty),
    Iter(Ty),
    OptionIs(bool),
    OptionUnwrap(Ty, bool),
    OptionUnwrapOr(Ty),
    OptionTake(Ty),
    OptionCopied(Ty),
    ResultIs(bool),
    ResultUnwrap(Ty, bool),
    ResultUnwrapOr(Ty),
    ResultSide(Ty, bool),
    IterSum(Ty),
    IterCount,
    IterMinMax(Ty, bool),
    IterRev(Ty),
    IterEnumerate(Ty),
    IterCollect(Ty),
    IterCopied(Ty),
    RangeContains,
}

impl Recv {
    fn new(l: &mut Lowerer, base: &Ty, method: &str, line: u32) -> R<Recv> {
        let no_method = |l: &Lowerer| {
            let shown = l.types.display(base);
            let (fr, en) = match (base, method) {
                (Ty::Adt(i), "clone") if matches!(l.types.adts[*i].kind, AdtKind::Opaque { .. }) => (
                    " : un objet du jeu est unique, il ne se copie pas (c'est tout l'intérêt de l'ownership !)".to_string(),
                    ": a game item is unique, it cannot be copied (that's the whole point of ownership!)".to_string(),
                ),
                (Ty::Adt(_), "clone") => (
                    " : ajoute #[derive(Clone)] au-dessus du type".to_string(),
                    ": add #[derive(Clone)] above the type".to_string(),
                ),
                (_, "iter_mut") => (
                    " ici : utilise-la directement dans une boucle for".to_string(),
                    " here: use it directly in a for loop".to_string(),
                ),
                (_, "map" | "filter" | "fold" | "any" | "all" | "find" | "position") => (
                    " (les fermetures |x| … arrivent plus tard : utilise une boucle for)".to_string(),
                    " (closures |x| … come later: use a for loop)".to_string(),
                ),
                _ => (String::new(), String::new()),
            };
            rerr(
                line,
                "E0599",
                format!("le type « {shown} » n'a pas de méthode « {method} »{fr}"),
                format!("the type '{shown}' has no method '{method}'{en}"),
            )
        };
        let clone_of = |l: &Lowerer, ty: &Ty| -> R<Recv> {
            if l.types.is_clone(ty) {
                Ok(Recv::Clone(ty.clone()))
            } else {
                Err(no_method(l))
            }
        };
        Ok(match (l.types.shallow(base), method) {
            (Ty::Var(_), _) => {
                return Err(rerr(
                    line,
                    "E0282",
                    format!(
                        "impossible d'appeler .{method}() : le type de cette valeur est encore inconnu. Précise-le, par exemple let v: Vec<i64> = Vec::new();"
                    ),
                    format!(
                        "cannot call .{method}(): the type of this value is not known yet. Specify it, for example let v: Vec<i64> = Vec::new();"
                    ),
                ));
            }
            (Ty::Adt(i), _) => {
                if let Some(sig) = l.fns.get(&format!("{i}::{method}")) {
                    if sig.self_kind.is_none() {
                        let shown = l.types.adts[i].name.clone();
                        return Err(rerr(
                            line,
                            "E0599",
                            format!(
                                "« {method} » n'a pas de self : appelle-la avec {shown}::{method}(…)"
                            ),
                            format!("'{method}' has no self: call it with {shown}::{method}(…)"),
                        ));
                    }
                    return Ok(Recv::User(sig.clone()));
                }
                if let AdtKind::Opaque { fields } = &l.types.adts[i].kind
                    && let Some(pos) = fields
                        .iter()
                        .position(|(names, _)| names.iter().any(|n| n == method))
                {
                    return Ok(Recv::Getter(pos as u32, fields[pos].1.clone()));
                }
                match method {
                    "clone" => clone_of(l, base)?,
                    _ => return Err(no_method(l)),
                }
            }
            (Ty::Int, "abs") => Recv::IntAbs,
            (Ty::Int, "pow") => Recv::IntPow,
            (Ty::Int, "min") => Recv::IntMinMax(false),
            (Ty::Int, "max") => Recv::IntMinMax(true),
            (Ty::Int, "clamp") => Recv::IntClamp,
            (Ty::Int, "abs_diff") => Recv::IntAbsDiff,
            (Ty::Int, "rem_euclid") => Recv::IntRemEuclid,
            (Ty::Int, "is_positive") => Recv::IntSign(true),
            (Ty::Int, "is_negative") => Recv::IntSign(false),
            (Ty::Int | Ty::Bool | Ty::Str | Ty::String, "to_string") => {
                Recv::ToString(base.clone())
            }
            (Ty::Int | Ty::Bool | Ty::Str | Ty::Unit, "clone") => Recv::Clone(base.clone()),
            (Ty::String, "clone") => Recv::Clone(Ty::String),
            (Ty::Str, "to_owned") | (Ty::String, "to_owned") => Recv::Clone(Ty::String),
            (Ty::Str | Ty::String, "len") => Recv::Len,
            (Ty::Str | Ty::String, "is_empty") => Recv::IsEmpty,
            (Ty::Str | Ty::String, "as_str") => Recv::AsStr,
            (Ty::Str | Ty::String, "chars") => Recv::Chars,
            (Ty::Str | Ty::String, "contains") => Recv::StrContains,
            (Ty::String, "push_str") => Recv::PushStr,
            (Ty::Vec(t), m) => match m {
                "len" => Recv::Len,
                "is_empty" => Recv::IsEmpty,
                "push" => Recv::VecPush(*t),
                "pop" => Recv::VecPop(*t),
                "remove" => Recv::VecRemove(*t),
                "insert" => Recv::VecInsert(*t),
                "clear" => Recv::VecClear,
                "sort" | "sort_unstable" => Recv::VecSort(*t),
                "reverse" => Recv::VecReverse,
                "swap" => Recv::VecSwap,
                "contains" => Recv::VecContains(*t),
                "first" => Recv::VecEnd(*t, false),
                "last" => Recv::VecEnd(*t, true),
                "get" => Recv::VecGet(*t),
                "iter" => Recv::Iter(Ty::Ref(false, t)),
                "into_iter" => Recv::Iter(*t),
                "clone" | "to_vec" => clone_of(l, base)?,
                _ => return Err(no_method(l)),
            },
            (Ty::Option(t), m) => match m {
                "is_some" => Recv::OptionIs(true),
                "is_none" => Recv::OptionIs(false),
                "unwrap" => Recv::OptionUnwrap(*t, false),
                "expect" => Recv::OptionUnwrap(*t, true),
                "unwrap_or" => Recv::OptionUnwrapOr(*t),
                "take" => Recv::OptionTake(*t),
                "copied" | "cloned" => {
                    let inner = l.types.strip(&t);
                    Recv::OptionCopied(inner)
                }
                "clone" => clone_of(l, base)?,
                _ => return Err(no_method(l)),
            },
            (Ty::Result(t, e), m) => match m {
                "is_ok" => Recv::ResultIs(true),
                "is_err" => Recv::ResultIs(false),
                "unwrap" => Recv::ResultUnwrap(*t, false),
                "expect" => Recv::ResultUnwrap(*t, true),
                "unwrap_or" => Recv::ResultUnwrapOr(*t),
                "ok" => Recv::ResultSide(*t, true),
                "err" => Recv::ResultSide(*e, false),
                "clone" => clone_of(l, base)?,
                _ => return Err(no_method(l)),
            },
            (Ty::Iter(t), m) => match m {
                "sum" => Recv::IterSum(l.types.strip(&t)),
                "count" => Recv::IterCount,
                "max" => Recv::IterMinMax(*t, true),
                "min" => Recv::IterMinMax(*t, false),
                "rev" => Recv::IterRev(*t),
                "enumerate" => Recv::IterEnumerate(*t),
                "collect" => Recv::IterCollect(*t),
                "copied" | "cloned" => Recv::IterCopied(*t),
                _ => return Err(no_method(l)),
            },
            (Ty::Tuple(_), "clone") => clone_of(l, base)?,
            (Ty::Range, "contains") => Recv::RangeContains,
            _ => return Err(no_method(l)),
        })
    }

    fn mode(&self) -> Mode {
        match self {
            Recv::PushStr
            | Recv::VecPush(_)
            | Recv::VecPop(_)
            | Recv::VecRemove(_)
            | Recv::VecInsert(_)
            | Recv::VecClear
            | Recv::VecSort(_)
            | Recv::VecReverse
            | Recv::VecSwap
            | Recv::OptionTake(_) => Mode::Mut,
            Recv::OptionUnwrap(..)
            | Recv::OptionUnwrapOr(_)
            | Recv::ResultUnwrap(..)
            | Recv::ResultUnwrapOr(_)
            | Recv::ResultSide(..)
            | Recv::IterSum(_)
            | Recv::IterRev(_)
            | Recv::IterEnumerate(_)
            | Recv::IterCollect(_) => Mode::Consume,
            Recv::Iter(t) if !matches!(t, Ty::Ref(..)) => Mode::Consume,
            _ => Mode::Read,
        }
    }

    /// Les arguments sont empruntés (`contains(&x)`) plutôt que pris.
    fn borrows_args(&self) -> bool {
        matches!(
            self,
            Recv::VecContains(_)
                | Recv::StrContains
                | Recv::RangeContains
                | Recv::OptionUnwrap(..)
                | Recv::ResultUnwrap(..)
        )
    }

    fn arg_count(&self) -> usize {
        match self {
            Recv::IntPow
            | Recv::IntMinMax(_)
            | Recv::IntAbsDiff
            | Recv::IntRemEuclid
            | Recv::StrContains
            | Recv::PushStr
            | Recv::VecPush(_)
            | Recv::VecRemove(_)
            | Recv::VecContains(_)
            | Recv::VecGet(_)
            | Recv::OptionUnwrapOr(_)
            | Recv::ResultUnwrapOr(_)
            | Recv::RangeContains => 1,
            Recv::OptionUnwrap(_, message) | Recv::ResultUnwrap(_, message) => {
                usize::from(*message)
            }
            Recv::IntClamp | Recv::VecInsert(_) | Recv::VecSwap => 2,
            _ => 0,
        }
    }
}

fn arith(op: BinOp) -> CBin {
    match op {
        BinOp::Add => CBin::Add,
        BinOp::Sub => CBin::Sub,
        BinOp::Mul => CBin::Mul,
        BinOp::Div => CBin::Div,
        BinOp::Rem => CBin::Rem,
        BinOp::Eq => CBin::Eq,
        BinOp::Ne => CBin::Ne,
        BinOp::Lt => CBin::Lt,
        BinOp::Le => CBin::Le,
        BinOp::Gt => CBin::Gt,
        BinOp::Ge => CBin::Ge,
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

enum Which {
    Next,
    Index(usize),
    Name(String),
}

enum Piece {
    Text(String),
    Arg { which: Which, debug: bool },
}

/// Découpe le texte d'un `println!` : `{}`, `{:?}`, `{0}`, `{nom}`, `{{` et `}}`.
fn parse_format(template: &str, line: u32) -> R<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                let mut inside = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(ch) => inside.push(ch),
                        None => {
                            return Err(rerr(
                                line,
                                "",
                                "« { » sans « } » dans le texte (pour écrire une accolade : {{)",
                                "'{' without '}' in the text (to write a brace: {{)",
                            ));
                        }
                    }
                }
                if !text.is_empty() {
                    pieces.push(Piece::Text(mem::take(&mut text)));
                }
                let (arg, spec) = inside.split_once(':').unwrap_or((inside.as_str(), ""));
                let arg = arg.trim();
                let which = if arg.is_empty() {
                    Which::Next
                } else if let Ok(i) = arg.parse::<usize>() {
                    Which::Index(i)
                } else {
                    Which::Name(arg.to_string())
                };
                pieces.push(Piece::Arg {
                    which,
                    debug: spec.contains('?'),
                });
            }
            '}' => {
                return Err(rerr(
                    line,
                    "",
                    "« } » sans « { » dans le texte (pour écrire une accolade : }})",
                    "'}' without '{' in the text (to write a brace: }})",
                ));
            }
            other => text.push(other),
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

fn suggest(wanted: &str, candidates: Vec<&str>) -> Option<String> {
    candidates
        .into_iter()
        .map(|c| (edit_distance(wanted, c), c))
        .filter(|(d, c)| *d <= 2.max(wanted.len() / 4) && *d < c.len() && *d > 0)
        .min()
        .map(|(_, c)| c.to_string())
}

fn hint_fr(suggestion: &Option<String>) -> String {
    suggestion
        .as_ref()
        .map_or(String::new(), |s| format!(" — voulais-tu écrire « {s} » ?"))
}

fn hint_en(suggestion: &Option<String>) -> String {
    suggestion
        .as_ref()
        .map_or(String::new(), |s| format!(" — did you mean '{s}'?"))
}
