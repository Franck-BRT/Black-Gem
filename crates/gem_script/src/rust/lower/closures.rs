//! Fermetures `|x| …`.
//!
//! - passée directement à un itérateur ou à une méthode (`.map(|x| x * 2)`), la fermeture est
//!   **recopiée sur place** : son corps est traduit à l'endroit de l'appel, il peut donc
//!   lire et modifier les variables autour (comme `FnMut`) ;
//! - rangée dans une variable (`let double = |x| x * 2;`) ou passée à une fonction, elle
//!   devient une vraie valeur : une fonction et une **copie** des variables capturées
//!   (avec `move`, les valeurs non copiables sont déplacées dans la fermeture).

use super::*;

/// Noms simples utilisés dans une expression (y compris dans le texte d'un `println!`).
pub(super) fn free_names(e: &Expr, names: &mut BTreeSet<String>) {
    let all = |list: &[Expr], names: &mut BTreeSet<String>| {
        for item in list {
            free_names(item, names);
        }
    };
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::Continue => {}
        ExprKind::Path(path) => {
            if path.len() == 1 {
                names.insert(path[0].clone());
            }
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => all(items, names),
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Binary(_, a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::CompoundAssign(_, a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssertEq(a, b, _) => {
            free_names(a, names);
            free_names(b, names);
        }
        ExprKind::StructLit(_, fields) => {
            for (_, value) in fields {
                free_names(value, names);
            }
        }
        ExprKind::Unary(_, inner)
        | ExprKind::Ref(_, inner)
        | ExprKind::Cast(inner, _)
        | ExprKind::Field(inner, _)
        | ExprKind::Try(inner) => free_names(inner, names),
        ExprKind::Call(callee, args) => {
            free_names(callee, names);
            all(args, names);
        }
        ExprKind::MethodCall(receiver, _, args) => {
            free_names(receiver, names);
            all(args, names);
        }
        ExprKind::Range(a, b, _) => {
            for side in [a, b].into_iter().flatten() {
                free_names(side, names);
            }
        }
        ExprKind::If(condition, then, otherwise) => {
            free_names(condition, names);
            block_names(then, names);
            if let Some(o) = otherwise {
                free_names(o, names);
            }
        }
        ExprKind::IfLet(_, scrutinee, then, otherwise) => {
            free_names(scrutinee, names);
            block_names(then, names);
            if let Some(o) = otherwise {
                free_names(o, names);
            }
        }
        ExprKind::While(condition, body) | ExprKind::WhileLet(_, condition, body) => {
            free_names(condition, names);
            block_names(body, names);
        }
        ExprKind::For(_, iterable, body) => {
            free_names(iterable, names);
            block_names(body, names);
        }
        ExprKind::Loop(body) | ExprKind::Block(body) => block_names(body, names),
        ExprKind::Match(scrutinee, arms) => {
            free_names(scrutinee, names);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    free_names(g, names);
                }
                free_names(&arm.body, names);
            }
        }
        ExprKind::Break(value) | ExprKind::Return(value) => {
            if let Some(v) = value {
                free_names(v, names);
            }
        }
        ExprKind::Format(_, template, args) | ExprKind::Write(_, template, args, _) => {
            if let ExprKind::Write(target, ..) = &e.kind {
                free_names(target, names);
            }
            all(args, names);
            if let Ok(pieces) = parse_format(template, 0) {
                for piece in pieces {
                    if let Piece::Arg {
                        which: Which::Name(n),
                        ..
                    } = piece
                    {
                        names.insert(n);
                    }
                }
            }
        }
        ExprKind::Assert(condition, message) => {
            free_names(condition, names);
            if let Some(m) = message {
                free_names(m, names);
            }
        }
        ExprKind::Closure { body, .. } => free_names(body, names),
    }
}

fn block_names(block: &Block, names: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Expr(e) => free_names(e, names),
            Stmt::Let {
                init, otherwise, ..
            } => {
                if let Some(i) = init {
                    free_names(i, names);
                }
                if let Some(o) = otherwise {
                    block_names(o, names);
                }
            }
        }
    }
    if let Some(tail) = &block.tail {
        free_names(tail, names);
    }
}

impl Lowerer<'_> {
    /// Une fermeture qui devient une valeur (fonction + copie des variables capturées).
    pub(super) fn lambda(
        &mut self,
        params: &[(Pat, Option<TypeExpr>)],
        ret: Option<&TypeExpr>,
        body: &Expr,
        is_move: bool,
        line: u32,
    ) -> R<(CExpr, Ty)> {
        let mut names = BTreeSet::new();
        free_names(body, &mut names);
        let captures: Vec<(String, usize)> = names
            .into_iter()
            .filter_map(|n| self.lookup(&n).map(|id| (n, id)))
            .collect();
        for (_, id) in &captures {
            let place = Place {
                local: *id,
                elems: Vec::new(),
                ty: self.f.locals[*id].ty.clone(),
                through: None,
                move_path: Vec::new(),
                blocked: None,
            };
            self.check_use(&place, line)?;
            if is_move {
                self.do_move(&place, line)?;
            }
        }
        let param_tys = params
            .iter()
            .map(|(_, t)| match t {
                Some(t) => self.ty(t, line),
                None => Ok(self.types.fresh()),
            })
            .collect::<R<Vec<_>>>()?;
        if param_tys
            .iter()
            .any(|t| matches!(self.types.shallow(t), Ty::Ref(true, _)))
        {
            return Err(unsupported(
                line,
                "un paramètre &mut dans une fermeture rangée dans une variable",
                "a &mut parameter in a closure stored in a variable",
            ));
        }
        let ret_ty = match ret {
            Some(t) => self.ty(t, line)?,
            None => self.types.fresh(),
        };
        let outer = mem::replace(&mut self.f, FnState::new(ret_ty.clone(), false));
        let mut captured = Vec::new();
        for (n, id) in &captures {
            let ty = outer.locals[*id].ty.clone();
            let slot = self.declare(n, ty, false);
            self.f.locals.last_mut().unwrap().capture = true;
            captured.push((slot, name(&outer.locals[*id].slot)));
        }
        let mut code = Vec::new();
        let mut slots = Vec::new();
        for ((pat, _), ty) in params.iter().zip(&param_tys) {
            match pat {
                Pat::Bind {
                    name: var,
                    mutable,
                    by_ref: false,
                } if !self.is_const_pattern(var) => {
                    slots.push(self.declare(var, ty.clone(), *mutable));
                }
                _ => {
                    let t = self.temp();
                    let mut binds = Vec::new();
                    if self
                        .pattern(pat, &name(&t), ty, &mut binds, None, false, line)?
                        .is_some()
                    {
                        return Err(rerr(
                            line,
                            "E0005",
                            "le motif d'un paramètre doit toujours correspondre",
                            "the pattern of a parameter must always match",
                        ));
                    }
                    self.bind(binds, None, line, &mut code)?;
                    slots.push(t);
                }
            }
        }
        let (value, ty) = self.value(body, &mut code)?;
        if !self.types.unify(&ret_ty, &ty) {
            return Err(self.mismatch(&ret_ty, &ty, body.line));
        }
        if self.f.flow.reachable {
            code.push(CStmt {
                line: body.line,
                kind: K::Return(Some(value)),
            });
        }
        self.f = outer;
        Ok((
            CExpr::Lambda {
                params: slots,
                captures: captured,
                body: code,
            },
            Ty::Fn(param_tys, Box::new(ret_ty)),
        ))
    }

    /// Applique une fermeture à des valeurs : recopiée sur place si c'est une fermeture
    /// écrite là (`|x| …`), sinon appelée (variable contenant une fermeture, nom de fonction).
    pub(super) fn apply_closure(
        &mut self,
        closure: &Expr,
        args: Vec<(CExpr, Ty)>,
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let ExprKind::Closure {
            params, ret, body, ..
        } = &closure.kind
        else {
            let (f, fty) = self.value(closure, out)?;
            let Ty::Fn(params, ret) = self.types.strip(&fty) else {
                return Err(rerr(
                    closure.line,
                    "E0618",
                    format!(
                        "une fermeture |x| … est attendue ici, pas « {} »",
                        self.types.display(&fty)
                    ),
                    format!(
                        "a closure |x| … is expected here, not '{}'",
                        self.types.display(&fty)
                    ),
                ));
            };
            if params.len() != args.len() {
                return Err(self.closure_arity(args.len(), params.len(), line));
            }
            let mut values = Vec::new();
            for (p, (v, t)) in params.iter().zip(args) {
                if !self.types.unify(p, &t) {
                    let (sp, st) = (self.types.strip(p), self.types.strip(&t));
                    if !self.types.unify(&sp, &st) {
                        return Err(self.mismatch(p, &t, line));
                    }
                }
                values.push(v);
            }
            return Ok((CExpr::Call(Box::new(f), values), *ret));
        };
        if params.len() != args.len() {
            return Err(self.closure_arity(args.len(), params.len(), line));
        }
        self.push_scope();
        let saved = self.f.inline_base.replace(self.f.loops.len());
        for ((pat, annotation), (v, t)) in params.iter().zip(args) {
            if let Some(a) = annotation {
                let at = self.ty(a, line)?;
                if !self.types.unify(&at, &t) {
                    return Err(self.mismatch(&at, &t, line));
                }
            }
            let v = self.spill(v, line, out);
            match pat {
                Pat::Bind {
                    name: var,
                    mutable,
                    by_ref: false,
                } if !self.is_const_pattern(var) => {
                    let slot = self.declare(var, t, *mutable);
                    out.push(CStmt {
                        line,
                        kind: K::Assign(Target::Name(slot), v),
                    });
                }
                _ => {
                    let mut binds = Vec::new();
                    if self
                        .pattern(pat, &v, &t, &mut binds, None, false, line)?
                        .is_some()
                    {
                        return Err(rerr(
                            line,
                            "E0005",
                            "le motif d'un paramètre de fermeture doit toujours correspondre",
                            "the pattern of a closure parameter must always match",
                        ));
                    }
                    self.bind(binds, None, line, out)?;
                }
            }
        }
        let (v, t) = self.value(body, out)?;
        if let Some(r) = ret {
            let rt = self.ty(r, line)?;
            self.expect(&rt, &t, body.line)?;
        }
        self.f.inline_base = saved;
        self.pop_scope();
        Ok((v, t))
    }

    fn closure_arity(&self, expected: usize, got: usize, line: u32) -> ScriptError {
        rerr(
            line,
            "E0593",
            format!("la fermeture doit prendre {expected} paramètre(s), elle en prend {got}"),
            format!("the closure must take {expected} parameter(s), it takes {got}"),
        )
    }
}
