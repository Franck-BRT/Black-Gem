//! Itérateurs : `v.iter().filter(|x| …).map(|x| …).sum()`.
//!
//! Une chaîne d'itérateur est traduite en **une seule boucle** : chaque élément traverse
//! les adaptateurs (`map`, `filter`, `take`…) l'un après l'autre, puis le consommateur
//! (`sum`, `collect`, `find`…). C'est le comportement « paresseux » de Rust : `find` s'arrête
//! au premier élément trouvé, `take(3)` ne regarde que ce qu'il faut.
//!
//! On y trouve aussi les méthodes à fermeture d'`Option` (`map`, `and_then`, `unwrap_or_else`…)
//! et de `Vec` (`retain`, `sort_by_key`).

use super::*;

const ADAPTERS: [&str; 14] = [
    "map",
    "filter",
    "filter_map",
    "enumerate",
    "rev",
    "take",
    "skip",
    "take_while",
    "skip_while",
    "step_by",
    "zip",
    "copied",
    "cloned",
    "inspect",
];

const CONSUMERS: [&str; 16] = [
    "collect",
    "sum",
    "product",
    "count",
    "min",
    "max",
    "min_by_key",
    "max_by_key",
    "any",
    "all",
    "find",
    "position",
    "fold",
    "for_each",
    "last",
    "nth",
];

const OPTION_CLOSURES: [&str; 7] = [
    "map",
    "and_then",
    "filter",
    "unwrap_or_else",
    "map_or",
    "is_some_and",
    "ok_or",
];

pub(super) fn is_adapter(method: &str) -> bool {
    ADAPTERS.contains(&method)
}

pub(super) fn is_consumer(method: &str) -> bool {
    CONSUMERS.contains(&method)
}

pub(super) fn is_option_closure(method: &str) -> bool {
    OPTION_CLOSURES.contains(&method)
}

/// D'où viennent les éléments.
#[derive(Clone, Copy)]
pub(super) enum Source<'e> {
    /// `v.iter()` : des références `&T`.
    Iter(&'e Expr),
    /// `v.into_iter()` : les valeurs (le vecteur est déplacé).
    IntoIter(&'e Expr),
    /// `texte.chars()`.
    Chars(&'e Expr),
    /// `a..b`.
    Range(&'e Expr),
    /// Une variable qui contient déjà un itérateur ou un intervalle.
    Value(&'e Expr),
}

#[derive(Clone)]
pub(super) struct Chain<'e> {
    source: Source<'e>,
    adapters: Vec<(&'e str, &'e [Expr], u32)>,
}

/// Ce qui consomme les éléments.
pub(super) enum Consumer<'e> {
    Collect,
    Sum,
    Product,
    Count,
    MinMax(bool),
    ByKey(&'e Expr, bool),
    Any(&'e Expr),
    All(&'e Expr),
    Find(&'e Expr),
    Position(&'e Expr),
    Fold(&'e Expr, &'e Expr),
    ForEach(&'e Expr),
    Last,
    Nth(&'e Expr),
    /// Une boucle `for motif in chaîne { corps }`.
    For(&'e Pat, &'e Block),
}

impl<'e> Consumer<'e> {
    pub(super) fn new(method: &str, args: &'e [Expr], line: u32) -> R<Consumer<'e>> {
        let wanted = match method {
            "fold" => 2,
            "min_by_key" | "max_by_key" | "any" | "all" | "find" | "position" | "for_each"
            | "nth" => 1,
            _ => 0,
        };
        if args.len() != wanted {
            return Err(rerr(
                line,
                "E0061",
                format!(
                    ".{method}() attend {wanted} argument(s), mais en reçoit {}",
                    args.len()
                ),
                format!(
                    ".{method}() takes {wanted} argument(s) but got {}",
                    args.len()
                ),
            ));
        }
        Ok(match method {
            "collect" => Consumer::Collect,
            "sum" => Consumer::Sum,
            "product" => Consumer::Product,
            "count" => Consumer::Count,
            "min" => Consumer::MinMax(false),
            "max" => Consumer::MinMax(true),
            "min_by_key" => Consumer::ByKey(&args[0], false),
            "max_by_key" => Consumer::ByKey(&args[0], true),
            "any" => Consumer::Any(&args[0]),
            "all" => Consumer::All(&args[0]),
            "find" => Consumer::Find(&args[0]),
            "position" => Consumer::Position(&args[0]),
            "fold" => Consumer::Fold(&args[0], &args[1]),
            "for_each" => Consumer::ForEach(&args[0]),
            "last" => Consumer::Last,
            _ => Consumer::Nth(&args[0]),
        })
    }
}

/// Préparation d'un adaptateur, avant la boucle.
enum Prep {
    Nothing,
    /// Compteur (et limite éventuelle).
    Counter(String, Option<CExpr>),
    Zip(String, String, Ty),
    Flag(String),
}

fn assign(line: u32, slot: &str, value: CExpr) -> CStmt {
    CStmt {
        line,
        kind: K::Assign(Target::Name(slot.to_string()), value),
    }
}

fn when(line: u32, condition: CExpr, then: Vec<CStmt>) -> CStmt {
    CStmt {
        line,
        kind: K::If(vec![(condition, then)], Vec::new()),
    }
}

fn jump(line: u32, kind: K) -> CStmt {
    CStmt { line, kind }
}

impl Lowerer<'_> {
    /// Reconnaît `source.adaptateur(…)…` (sans le consommateur final).
    pub(super) fn iterator_chain<'e>(&self, e: &'e Expr) -> Option<Chain<'e>> {
        let mut adapters = Vec::new();
        let mut current = e;
        while let ExprKind::MethodCall(receiver, method, args) = &current.kind {
            if !is_adapter(method) {
                break;
            }
            adapters.push((method.as_str(), args.as_slice(), current.line));
            current = receiver;
        }
        adapters.reverse();
        let source = match &current.kind {
            ExprKind::MethodCall(receiver, method, args) if args.is_empty() => {
                match method.as_str() {
                    "iter" => Source::Iter(receiver),
                    "into_iter" => Source::IntoIter(receiver),
                    "chars" => Source::Chars(receiver),
                    _ => return None,
                }
            }
            ExprKind::Range(Some(_), Some(_), _) => Source::Range(current),
            ExprKind::Path(path) if path.len() == 1 => {
                let id = self.lookup(&path[0])?;
                match self.types.strip(&self.f.locals[id].ty) {
                    Ty::Iter(_) | Ty::Range => Source::Value(current),
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(Chain { source, adapters })
    }

    fn chain_source(&mut self, source: Source, line: u32, out: &mut Vec<CStmt>) -> R<(CExpr, Ty)> {
        let no_iter = |l: &Self, t: &Ty, what: &str| {
            rerr(
                line,
                "E0599",
                format!("« {} » n'a pas de méthode .{what}()", l.types.display(t)),
                format!("'{}' has no method .{what}()", l.types.display(t)),
            )
        };
        let unknown = || {
            rerr(
                line,
                "E0282",
                "le type de ce que parcourt l'itérateur est encore inconnu : précise-le",
                "the type of what the iterator goes through is not known yet: specify it",
            )
        };
        Ok(match source {
            Source::Iter(x) => {
                let (v, t) = self.expr(x, out)?;
                match self.types.strip(&t) {
                    Ty::Vec(elem) => (v, Ty::Ref(false, elem)),
                    Ty::Iter(elem) => (v, *elem),
                    Ty::Var(_) => return Err(unknown()),
                    other => return Err(no_iter(self, &other, "iter")),
                }
            }
            Source::IntoIter(x) => {
                let (v, t) = self.value(x, out)?;
                match self.types.shallow(&t) {
                    Ty::Vec(elem) | Ty::Iter(elem) => (v, *elem),
                    Ty::Range => (v, Ty::Int),
                    Ty::Ref(m, inner) => match self.types.shallow(&inner) {
                        Ty::Vec(elem) => (v, Ty::Ref(m, elem)),
                        Ty::Iter(elem) => (v, *elem),
                        other => return Err(no_iter(self, &other, "into_iter")),
                    },
                    Ty::Var(_) => return Err(unknown()),
                    other => return Err(no_iter(self, &other, "into_iter")),
                }
            }
            Source::Chars(x) => {
                let (v, t) = self.expr(x, out)?;
                match self.types.strip(&t) {
                    Ty::Str | Ty::String => (v, Ty::Str),
                    other => return Err(no_iter(self, &other, "chars")),
                }
            }
            Source::Range(r) => {
                let (v, _) = self.expr_inner(r, out)?;
                (v, Ty::Int)
            }
            Source::Value(x) => {
                let (v, t) = self.value(x, out)?;
                match self.types.strip(&t) {
                    Ty::Iter(elem) => (v, *elem),
                    _ => (v, Ty::Int),
                }
            }
        })
    }

    /// Évalue une chaîne avec son consommateur : une seule boucle.
    pub(super) fn lower_chain(
        &mut self,
        chain: &Chain,
        consumer: Consumer,
        as_iterator: bool,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let (source, item) = self.chain_source(chain.source, line, out)?;
        // `rev()` retourne la source (avant tout adaptateur qui compte les éléments).
        let mut reversed = false;
        let mut adapters: Vec<(&str, &[Expr], u32)> = Vec::new();
        for &(m, args, at) in &chain.adapters {
            if m == "rev" {
                if adapters.iter().any(|(k, _, _)| {
                    !matches!(
                        *k,
                        "map" | "filter" | "filter_map" | "copied" | "cloned" | "inspect"
                    )
                }) {
                    return Err(unsupported(
                        at,
                        "rev() après enumerate, take, skip, zip ou step_by (place rev() plus tôt)",
                        "rev() after enumerate, take, skip, zip or step_by (put rev() earlier)",
                    ));
                }
                reversed = !reversed;
            } else {
                adapters.push((m, args, at));
            }
        }
        let source = if reversed {
            CExpr::BuiltinCall(Builtin::Reverse, vec![source])
        } else {
            source
        };
        // Arguments des adaptateurs, évalués une fois, avant la boucle.
        let mut preps = Vec::new();
        for &(m, args, at) in &adapters {
            let one = |l: &Self| -> R<()> {
                if args.len() != 1 {
                    return Err(l.arg_count(&format!(".{m}()"), 1, args.len(), at));
                }
                Ok(())
            };
            preps.push(match m {
                "take" | "skip" | "step_by" => {
                    one(self)?;
                    let (n, t) = self.value(&args[0], out)?;
                    self.expect_int(&t, at, m)?;
                    let n = self.spill(n, at, out);
                    let counter = self.temp();
                    out.push(assign(at, &counter, int(0)));
                    Prep::Counter(counter, Some(n))
                }
                "enumerate" => {
                    let counter = self.temp();
                    out.push(assign(at, &counter, int(0)));
                    Prep::Counter(counter, None)
                }
                "zip" => {
                    one(self)?;
                    let (list, ty) = match self.iterator_chain(&args[0]) {
                        Some(other) => {
                            self.lower_chain(&other, Consumer::Collect, false, at, out)?
                        }
                        None => self.value(&args[0], out)?,
                    };
                    let item = self.list_item(&ty, at)?;
                    let (z, i) = (self.temp(), self.temp());
                    out.push(assign(at, &z, list));
                    out.push(assign(at, &i, int(0)));
                    Prep::Zip(z, i, item)
                }
                "skip_while" => {
                    one(self)?;
                    let flag = self.temp();
                    out.push(assign(at, &flag, CExpr::Bool(true)));
                    Prep::Flag(flag)
                }
                "map" | "filter" | "filter_map" | "take_while" | "inspect" => {
                    one(self)?;
                    Prep::Nothing
                }
                _ => Prep::Nothing,
            });
        }
        // Valeur du consommateur, avant la boucle.
        let acc = self.temp();
        let extra = self.temp();
        let mut fold_ty = None;
        match &consumer {
            Consumer::Collect => out.push(assign(line, &acc, CExpr::List(Vec::new()))),
            Consumer::Sum | Consumer::Count => out.push(assign(line, &acc, int(0))),
            Consumer::Product => out.push(assign(line, &acc, int(1))),
            Consumer::MinMax(_) | Consumer::Find(_) | Consumer::Last => {
                out.push(assign(line, &acc, CExpr::None))
            }
            Consumer::ByKey(..) => {
                out.push(assign(line, &acc, CExpr::None));
                out.push(assign(line, &extra, CExpr::None));
            }
            Consumer::Any(_) => out.push(assign(line, &acc, CExpr::Bool(false))),
            Consumer::All(_) => out.push(assign(line, &acc, CExpr::Bool(true))),
            Consumer::Position(_) => {
                out.push(assign(line, &acc, CExpr::None));
                out.push(assign(line, &extra, int(0)));
            }
            Consumer::Nth(n) => {
                let (nv, nt) = self.value(n, out)?;
                self.expect_int(&nt, line, "nth")?;
                out.push(assign(line, &acc, CExpr::None));
                out.push(assign(line, &extra, int(0)));
                let limit = self.spill(nv, line, out);
                out.push(assign(line, &format!("{extra}n"), limit));
            }
            Consumer::Fold(init, _) => {
                let (v, t) = self.value(init, out)?;
                out.push(assign(line, &acc, v));
                fold_ty = Some(t);
            }
            Consumer::ForEach(_) | Consumer::For(..) => {}
        }

        let entry = self.begin_loop(false);
        self.push_scope();
        let element = self.temp();
        let mut code = Vec::new();
        let mut cur = name(&element);
        let mut cur_ty = item;
        for (prep, &(m, args, at)) in preps.iter().zip(&adapters) {
            match (m, prep) {
                ("map", _) => {
                    let (v, t) = self.apply_closure(
                        &args[0],
                        vec![(cur.clone(), cur_ty.clone())],
                        at,
                        &mut code,
                    )?;
                    let s = self.temp();
                    code.push(assign(at, &s, v));
                    cur = name(&s);
                    cur_ty = t;
                }
                ("filter" | "take_while", _) => {
                    let arg = (cur.clone(), Ty::Ref(false, Box::new(cur_ty.clone())));
                    let (c, t) = self.apply_closure(&args[0], vec![arg], at, &mut code)?;
                    self.expect_bool(&t, at)?;
                    let exit = if m == "filter" { K::Continue } else { K::Break };
                    code.push(when(at, not(c), vec![jump(at, exit)]));
                }
                ("inspect", _) => {
                    let arg = (cur.clone(), Ty::Ref(false, Box::new(cur_ty.clone())));
                    let (v, _) = self.apply_closure(&args[0], vec![arg], at, &mut code)?;
                    self.discard(v, at, &mut code);
                }
                ("filter_map", _) => {
                    let (v, t) = self.apply_closure(
                        &args[0],
                        vec![(cur.clone(), cur_ty.clone())],
                        at,
                        &mut code,
                    )?;
                    let inner = self.types.fresh();
                    if !self.types.unify(&t, &Ty::Option(Box::new(inner.clone()))) {
                        return Err(self.mismatch(&Ty::Option(Box::new(inner)), &t, at));
                    }
                    let s = self.temp();
                    code.push(assign(at, &s, v));
                    code.push(when(
                        at,
                        bin(CBin::Eq, name(&s), CExpr::None),
                        vec![jump(at, K::Continue)],
                    ));
                    cur = name(&s);
                    cur_ty = inner;
                }
                ("skip_while", Prep::Flag(flag)) => {
                    let mut inner = Vec::new();
                    let arg = (cur.clone(), Ty::Ref(false, Box::new(cur_ty.clone())));
                    let (c, t) = self.apply_closure(&args[0], vec![arg], at, &mut inner)?;
                    self.expect_bool(&t, at)?;
                    inner.push(when(at, c, vec![jump(at, K::Continue)]));
                    inner.push(assign(at, flag, CExpr::Bool(false)));
                    code.push(when(at, name(flag), inner));
                }
                ("enumerate", Prep::Counter(counter, _)) => {
                    let s = self.temp();
                    code.push(assign(
                        at,
                        &s,
                        CExpr::Tuple(vec![name(counter), cur.clone()]),
                    ));
                    code.push(assign(at, counter, bin(CBin::Add, name(counter), int(1))));
                    cur = name(&s);
                    cur_ty = Ty::Tuple(vec![Ty::Int, cur_ty]);
                }
                ("take", Prep::Counter(counter, Some(n))) => {
                    code.push(when(
                        at,
                        bin(CBin::Ge, name(counter), n.clone()),
                        vec![jump(at, K::Break)],
                    ));
                    code.push(assign(at, counter, bin(CBin::Add, name(counter), int(1))));
                }
                ("skip", Prep::Counter(counter, Some(n))) => {
                    code.push(when(
                        at,
                        bin(CBin::Lt, name(counter), n.clone()),
                        vec![
                            assign(at, counter, bin(CBin::Add, name(counter), int(1))),
                            jump(at, K::Continue),
                        ],
                    ));
                }
                ("step_by", Prep::Counter(counter, Some(k))) => {
                    let q = self.temp();
                    code.push(assign(at, &q, name(counter)));
                    code.push(assign(at, counter, bin(CBin::Add, name(counter), int(1))));
                    code.push(when(
                        at,
                        bin(CBin::Ne, bin(CBin::Rem, name(&q), k.clone()), int(0)),
                        vec![jump(at, K::Continue)],
                    ));
                }
                ("zip", Prep::Zip(list, i, other)) => {
                    let other = other.clone();
                    code.push(when(
                        at,
                        bin(
                            CBin::Ge,
                            name(i),
                            CExpr::BuiltinCall(Builtin::Len, vec![name(list)]),
                        ),
                        vec![jump(at, K::Break)],
                    ));
                    let s = self.temp();
                    code.push(assign(
                        at,
                        &s,
                        CExpr::Tuple(vec![cur.clone(), index(name(list), name(i))]),
                    ));
                    code.push(assign(at, i, bin(CBin::Add, name(i), int(1))));
                    cur = name(&s);
                    cur_ty = Ty::Tuple(vec![cur_ty, other]);
                }
                ("copied" | "cloned", _) => {
                    let inner = match self.types.shallow(&cur_ty) {
                        Ty::Ref(_, inner) => *inner,
                        other => other,
                    };
                    if m == "copied" && !self.types.is_copy(&inner) {
                        return Err(rerr(
                            at,
                            "E0277",
                            format!(
                                "copied() copie les éléments, mais « {} » n'est pas Copy : utilise cloned()",
                                self.types.display(&inner)
                            ),
                            format!(
                                "copied() copies the items, but '{}' is not Copy: use cloned()",
                                self.types.display(&inner)
                            ),
                        ));
                    }
                    if !self.types.is_clone(&inner) {
                        return Err(rerr(
                            at,
                            "E0277",
                            format!("« {} » ne peut pas être cloné", self.types.display(&inner)),
                            format!("'{}' cannot be cloned", self.types.display(&inner)),
                        ));
                    }
                    cur_ty = inner;
                }
                _ => unreachable!("adaptateur {m}"),
            }
        }
        // Le consommateur reçoit l'élément.
        let ordered = |l: &mut Self, ty: &Ty, what: &str| -> R<()> {
            match l.types.strip(ty) {
                Ty::Int | Ty::Str | Ty::String => Ok(()),
                Ty::Var(v) => {
                    l.types.unify(&Ty::Var(v), &Ty::Int);
                    Ok(())
                }
                other => Err(rerr(
                    line,
                    "E0277",
                    format!(
                        "{what} compare des nombres ou des textes, pas « {} » (essaie min_by_key / max_by_key)",
                        l.types.display(&other)
                    ),
                    format!(
                        "{what} compares numbers or texts, not '{}' (try min_by_key / max_by_key)",
                        l.types.display(&other)
                    ),
                )),
            }
        };
        let result_ty = match consumer {
            Consumer::For(pat, body) => {
                match pat {
                    Pat::Bind {
                        name: var,
                        mutable,
                        by_ref: false,
                    } if !self.is_const_pattern(var) => {
                        let slot = self.declare(var, cur_ty.clone(), *mutable);
                        code.push(assign(line, &slot, cur.clone()));
                    }
                    _ => {
                        let mut binds = Vec::new();
                        if self
                            .pattern(pat, &cur, &cur_ty, &mut binds, None, false, line)?
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
                    }
                }
                let (bv, _) = self.block(body, &mut code)?;
                self.discard(bv, body.end_line, &mut code);
                Ty::Unit
            }
            Consumer::Collect => {
                code.push(jump(
                    line,
                    K::Expr(CExpr::Method(
                        acc.clone(),
                        "append".into(),
                        vec![cur.clone()],
                    )),
                ));
                if as_iterator {
                    Ty::Iter(Box::new(cur_ty))
                } else {
                    Ty::Vec(Box::new(cur_ty))
                }
            }
            Consumer::Sum | Consumer::Product => {
                self.expect_int(&cur_ty, line, "sum / product")?;
                let op = if matches!(consumer, Consumer::Sum) {
                    CBin::Add
                } else {
                    CBin::Mul
                };
                code.push(assign(line, &acc, bin(op, name(&acc), cur.clone())));
                Ty::Int
            }
            Consumer::Count => {
                code.push(assign(line, &acc, bin(CBin::Add, name(&acc), int(1))));
                Ty::Int
            }
            Consumer::MinMax(max) => {
                ordered(self, &cur_ty, if max { "max()" } else { "min()" })?;
                // max garde le dernier des plus grands, min le premier des plus petits (comme Rust).
                let better = bin(
                    if max { CBin::Ge } else { CBin::Lt },
                    cur.clone(),
                    name(&acc),
                );
                let condition = CExpr::Or(
                    Box::new(bin(CBin::Eq, name(&acc), CExpr::None)),
                    Box::new(better),
                );
                code.push(when(line, condition, vec![assign(line, &acc, cur.clone())]));
                Ty::Option(Box::new(cur_ty))
            }
            Consumer::ByKey(f, max) => {
                let arg = (cur.clone(), Ty::Ref(false, Box::new(cur_ty.clone())));
                let (k, kt) = self.apply_closure(f, vec![arg], line, &mut code)?;
                ordered(self, &kt, if max { "max_by_key" } else { "min_by_key" })?;
                let key = self.temp();
                code.push(assign(line, &key, k));
                let better = bin(
                    if max { CBin::Ge } else { CBin::Lt },
                    name(&key),
                    name(&extra),
                );
                let condition = CExpr::Or(
                    Box::new(bin(CBin::Eq, name(&acc), CExpr::None)),
                    Box::new(better),
                );
                code.push(when(
                    line,
                    condition,
                    vec![
                        assign(line, &acc, cur.clone()),
                        assign(line, &extra, name(&key)),
                    ],
                ));
                Ty::Option(Box::new(cur_ty))
            }
            Consumer::Any(f) | Consumer::All(f) => {
                let any = matches!(consumer, Consumer::Any(_));
                let (c, t) =
                    self.apply_closure(f, vec![(cur.clone(), cur_ty.clone())], line, &mut code)?;
                self.expect_bool(&t, line)?;
                let condition = if any { c } else { not(c) };
                code.push(when(
                    line,
                    condition,
                    vec![assign(line, &acc, CExpr::Bool(any)), jump(line, K::Break)],
                ));
                Ty::Bool
            }
            Consumer::Find(f) => {
                let arg = (cur.clone(), Ty::Ref(false, Box::new(cur_ty.clone())));
                let (c, t) = self.apply_closure(f, vec![arg], line, &mut code)?;
                self.expect_bool(&t, line)?;
                code.push(when(
                    line,
                    c,
                    vec![assign(line, &acc, cur.clone()), jump(line, K::Break)],
                ));
                Ty::Option(Box::new(cur_ty))
            }
            Consumer::Position(f) => {
                let (c, t) =
                    self.apply_closure(f, vec![(cur.clone(), cur_ty.clone())], line, &mut code)?;
                self.expect_bool(&t, line)?;
                code.push(when(
                    line,
                    c,
                    vec![assign(line, &acc, name(&extra)), jump(line, K::Break)],
                ));
                code.push(assign(line, &extra, bin(CBin::Add, name(&extra), int(1))));
                Ty::Option(Box::new(Ty::Int))
            }
            Consumer::Fold(_, f) => {
                let acc_ty = fold_ty.unwrap_or(Ty::Unit);
                let (v, t) = self.apply_closure(
                    f,
                    vec![(name(&acc), acc_ty.clone()), (cur.clone(), cur_ty.clone())],
                    line,
                    &mut code,
                )?;
                self.expect(&acc_ty, &t, line)?;
                code.push(assign(line, &acc, v));
                acc_ty
            }
            Consumer::ForEach(f) => {
                let (v, _) =
                    self.apply_closure(f, vec![(cur.clone(), cur_ty.clone())], line, &mut code)?;
                self.discard(v, line, &mut code);
                Ty::Unit
            }
            Consumer::Last => {
                code.push(assign(line, &acc, cur.clone()));
                Ty::Option(Box::new(cur_ty))
            }
            Consumer::Nth(_) => {
                code.push(when(
                    line,
                    bin(CBin::Eq, name(&extra), name(&format!("{extra}n"))),
                    vec![assign(line, &acc, cur.clone()), jump(line, K::Break)],
                ));
                code.push(assign(line, &extra, bin(CBin::Add, name(&extra), int(1))));
                Ty::Option(Box::new(cur_ty))
            }
        };
        self.pop_scope();
        self.end_loop(entry)?;
        out.push(CStmt {
            line,
            kind: K::For(Target::Name(element), source, code),
        });
        let value = if matches!(result_ty, Ty::Unit) {
            unit()
        } else {
            name(&acc)
        };
        Ok((value, result_ty))
    }

    /// Type des éléments d'une liste (pour `zip`).
    fn list_item(&mut self, ty: &Ty, line: u32) -> R<Ty> {
        Ok(match self.types.shallow(ty) {
            Ty::Vec(inner) | Ty::Iter(inner) => *inner,
            Ty::Ref(m, inner) => match self.types.shallow(&inner) {
                Ty::Vec(e) => Ty::Ref(m, e),
                other => return Err(self.not_iterable(&other, line)),
            },
            Ty::Range => Ty::Int,
            other => return Err(self.not_iterable(&other, line)),
        })
    }

    /// `Option` : `map`, `and_then`, `filter`, `unwrap_or_else`, `map_or`, `is_some_and`,
    /// `ok_or`. L'option est consommée (déplacée).
    pub(super) fn option_closure(
        &mut self,
        method: &str,
        option: CExpr,
        inner: Ty,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let wanted = if method == "map_or" { 2 } else { 1 };
        if args.len() != wanted {
            return Err(self.arg_count(&format!(".{method}()"), wanted, args.len(), line));
        }
        let o = self.spill(option, line, out);
        let r = self.temp();
        let some = bin(CBin::Ne, o.clone(), CExpr::None);
        let before = self.f.flow.clone();
        let mut branch = Vec::new();
        let result = match method {
            "ok_or" => {
                let (e, et) = self.value(&args[0], out)?;
                out.push(assign(line, &r, CExpr::Tuple(vec![CExpr::Bool(false), e])));
                branch.push(assign(
                    line,
                    &r,
                    CExpr::Tuple(vec![CExpr::Bool(true), o.clone()]),
                ));
                Ty::Result(Box::new(inner), Box::new(et))
            }
            "map" | "and_then" => {
                out.push(assign(line, &r, CExpr::None));
                let (v, t) =
                    self.apply_closure(&args[0], vec![(o.clone(), inner)], line, &mut branch)?;
                branch.push(assign(line, &r, v));
                if method == "map" {
                    Ty::Option(Box::new(t))
                } else {
                    let u = self.types.fresh();
                    if !self.types.unify(&t, &Ty::Option(Box::new(u.clone()))) {
                        return Err(self.mismatch(&Ty::Option(Box::new(u)), &t, line));
                    }
                    t
                }
            }
            "filter" => {
                out.push(assign(line, &r, CExpr::None));
                let arg = (o.clone(), Ty::Ref(false, Box::new(inner.clone())));
                let (c, t) = self.apply_closure(&args[0], vec![arg], line, &mut branch)?;
                self.expect_bool(&t, line)?;
                branch.push(when(line, c, vec![assign(line, &r, o.clone())]));
                Ty::Option(Box::new(inner))
            }
            "is_some_and" => {
                out.push(assign(line, &r, CExpr::Bool(false)));
                let (c, t) =
                    self.apply_closure(&args[0], vec![(o.clone(), inner)], line, &mut branch)?;
                self.expect_bool(&t, line)?;
                branch.push(assign(line, &r, c));
                Ty::Bool
            }
            "map_or" => {
                let (d, dt) = self.value(&args[0], out)?;
                out.push(assign(line, &r, d));
                let (v, t) =
                    self.apply_closure(&args[1], vec![(o.clone(), inner)], line, &mut branch)?;
                self.expect(&dt, &t, line)?;
                branch.push(assign(line, &r, v));
                dt
            }
            _ => {
                // unwrap_or_else : la fermeture ne s'exécute que pour None.
                out.push(assign(line, &r, o.clone()));
                let (v, t) = self.apply_closure(&args[0], Vec::new(), line, &mut branch)?;
                self.expect(&inner, &t, line)?;
                branch.push(assign(line, &r, v));
                let after = mem::replace(&mut self.f.flow, before);
                self.f.flow = merge(self.f.flow.clone(), after);
                out.push(when(line, bin(CBin::Eq, o, CExpr::None), branch));
                return Ok((name(&r), inner));
            }
        };
        let after = mem::replace(&mut self.f.flow, before);
        self.f.flow = merge(self.f.flow.clone(), after);
        out.push(when(line, some, branch));
        Ok((name(&r), result))
    }

    /// `v.retain(|x| …)` et `v.sort_by_key(|x| …)` : le vecteur est modifié.
    pub(super) fn vec_closure(
        &mut self,
        method: &str,
        place: &Place,
        elem: Ty,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        if args.len() != 1 {
            return Err(self.arg_count(&format!(".{method}()"), 1, args.len(), line));
        }
        let list = self.place_expr(place);
        let list = self.spill(list, line, out);
        let kept = self.temp();
        out.push(assign(line, &kept, CExpr::List(Vec::new())));
        let entry = self.begin_loop(false);
        self.push_scope();
        let element = self.temp();
        let mut code = Vec::new();
        let arg = (name(&element), Ty::Ref(false, Box::new(elem.clone())));
        let (v, t) = self.apply_closure(&args[0], vec![arg], line, &mut code)?;
        let position = self.temp();
        if method == "retain" {
            self.expect_bool(&t, line)?;
            code.push(when(
                line,
                v,
                vec![jump(
                    line,
                    K::Expr(CExpr::Method(
                        kept.clone(),
                        "append".into(),
                        vec![name(&element)],
                    )),
                )],
            ));
        } else {
            if !matches!(
                self.types.strip(&t),
                Ty::Int | Ty::Str | Ty::String | Ty::Tuple(_) | Ty::Var(_)
            ) {
                return Err(rerr(
                    line,
                    "E0277",
                    format!(
                        "sort_by_key trie selon une clé nombre ou texte, pas « {} »",
                        self.types.display(&t)
                    ),
                    format!(
                        "sort_by_key sorts by a number or text key, not '{}'",
                        self.types.display(&t)
                    ),
                ));
            }
            // (clé, position, élément) : le tri reste stable et ne compare jamais les éléments.
            code.push(jump(
                line,
                K::Expr(CExpr::Method(
                    kept.clone(),
                    "append".into(),
                    vec![CExpr::Tuple(vec![v, name(&position), name(&element)])],
                )),
            ));
            code.push(assign(
                line,
                &position,
                bin(CBin::Add, name(&position), int(1)),
            ));
        }
        self.pop_scope();
        self.end_loop(entry)?;
        if method == "sort_by_key" {
            out.push(assign(line, &position, int(0)));
        }
        out.push(CStmt {
            line,
            kind: K::For(Target::Name(element), list, code),
        });
        let result = if method == "retain" {
            name(&kept)
        } else {
            let sorted = self.temp();
            let pair = self.temp();
            out.push(assign(line, &sorted, CExpr::List(Vec::new())));
            out.push(CStmt {
                line,
                kind: K::For(
                    Target::Name(pair.clone()),
                    CExpr::BuiltinCall(Builtin::Sort, vec![name(&kept)]),
                    vec![jump(
                        line,
                        K::Expr(CExpr::Method(
                            sorted.clone(),
                            "append".into(),
                            vec![index(name(&pair), int(2))],
                        )),
                    )],
                ),
            });
            name(&sorted)
        };
        self.store(place, result, line, out);
        Ok((unit(), Ty::Unit))
    }
}
