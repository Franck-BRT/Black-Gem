//! Traits et fonctions génériques.
//!
//! - `trait Nom { … }` déclare des méthodes (éventuellement avec un corps par défaut) ;
//!   `impl Nom for Type { … }` les fournit. `impl fmt::Display for Type` permet d'afficher
//!   le type avec `{}`.
//! - Une fonction générique (`fn f<T: Nom>(x: &T)`, ou `x: &impl Nom`) est **recopiée pour
//!   chaque type** avec lequel elle est appelée (monomorphisation, comme rustc) : chaque
//!   copie appelle directement les bonnes méthodes.

use super::*;

/// Traits fournis par Rust qu'on obtient avec `#[derive(…)]`.
const DERIVABLE: [&str; 6] = ["Debug", "Clone", "Copy", "PartialEq", "Eq", "Default"];

/// Une copie de fonction générique à fabriquer.
#[derive(Clone)]
pub(super) struct Instance {
    /// Nom caché de cet appel (plusieurs appels de mêmes types partagent une copie).
    slot: String,
    decl: FnDecl,
    /// Types (encore à déduire) des paramètres génériques nommés, puis anonymes (`impl T`).
    named: Vec<(String, Ty, Vec<Bound>)>,
    anonymous: Vec<(Ty, Vec<Bound>)>,
    line: u32,
}

impl Lowerer<'_> {
    pub(super) fn declare_traits(&mut self, items: &[Item]) -> R<()> {
        for item in items {
            let Item::Trait {
                name: trait_name,
                line,
                methods,
            } = item
            else {
                continue;
            };
            if self.traits.contains_key(trait_name)
                || DERIVABLE.contains(&trait_name.as_str())
                || trait_name == "Display"
            {
                return Err(rerr(
                    *line,
                    "E0428",
                    format!("le trait « {trait_name} » existe déjà"),
                    format!("the trait '{trait_name}' already exists"),
                ));
            }
            for m in methods {
                if !m.generics.is_empty() {
                    return Err(unsupported(
                        m.line,
                        "les méthodes génériques dans un trait",
                        "generic methods in a trait",
                    ));
                }
            }
            self.traits.insert(trait_name.clone(), methods.clone());
        }
        Ok(())
    }

    /// Méthodes d'un bloc `impl` : les siennes, plus celles que le trait fournit par défaut.
    pub(super) fn impl_methods(&self, trait_name: Option<&str>, methods: &[FnDecl]) -> Vec<FnDecl> {
        let mut all = methods.to_vec();
        if let Some(t) = trait_name
            && let Some(defaults) = self.traits.get(t)
        {
            for d in defaults {
                if d.has_body && !methods.iter().any(|m| m.name == d.name) {
                    all.push(d.clone());
                }
            }
        }
        all
    }

    /// Vérifie `impl Trait for Type` : méthodes demandées présentes, aucune en trop.
    pub(super) fn check_impl(
        &mut self,
        trait_name: &str,
        adt: usize,
        methods: &[FnDecl],
        line: u32,
    ) -> R<()> {
        let type_name = self.types.adts[adt].name.clone();
        if trait_name == "Display" {
            if !methods.iter().any(|m| m.name == "fmt") || methods.len() != 1 {
                return Err(rerr(
                    line,
                    "E0046",
                    "impl Display demande une seule méthode : fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result",
                    "impl Display needs a single method: fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result",
                ));
            }
            self.types.adts[adt].display = true;
            self.impls.insert((trait_name.to_string(), adt));
            return Ok(());
        }
        if DERIVABLE.contains(&trait_name) {
            return Err(rerr(
                line,
                "",
                format!(
                    "pour {trait_name}, écris plutôt #[derive({trait_name})] au-dessus de « {type_name} »"
                ),
                format!(
                    "for {trait_name}, write #[derive({trait_name})] above '{type_name}' instead"
                ),
            ));
        }
        let Some(declared) = self.traits.get(trait_name).cloned() else {
            let candidates: Vec<&str> = self.traits.keys().map(String::as_str).collect();
            let suggestion = suggest(trait_name, candidates);
            return Err(rerr(
                line,
                "E0405",
                format!(
                    "le trait « {trait_name} » est inconnu{}",
                    hint_fr(&suggestion)
                ),
                format!(
                    "the trait '{trait_name}' is unknown{}",
                    hint_en(&suggestion)
                ),
            ));
        };
        for m in methods {
            if !declared.iter().any(|d| d.name == m.name) {
                return Err(rerr(
                    m.line,
                    "E0407",
                    format!(
                        "la méthode « {} » ne fait pas partie du trait « {trait_name} »",
                        m.name
                    ),
                    format!(
                        "the method '{}' is not part of the trait '{trait_name}'",
                        m.name
                    ),
                ));
            }
        }
        for d in &declared {
            if !d.has_body && !methods.iter().any(|m| m.name == d.name) {
                return Err(rerr(
                    line,
                    "E0046",
                    format!(
                        "il manque la méthode « {} » du trait « {trait_name} » pour « {type_name} »",
                        d.name
                    ),
                    format!(
                        "the method '{}' of the trait '{trait_name}' is missing for '{type_name}'",
                        d.name
                    ),
                ));
            }
        }
        if !self.impls.insert((trait_name.to_string(), adt)) {
            return Err(rerr(
                line,
                "E0119",
                format!("« {trait_name} » est déjà implémenté pour « {type_name} »"),
                format!("'{trait_name}' is already implemented for '{type_name}'"),
            ));
        }
        Ok(())
    }

    /// La fonction a-t-elle des paramètres de type (`<T>` ou `impl Trait`) ?
    pub(super) fn is_generic(decl: &FnDecl) -> bool {
        !decl.generics.is_empty()
            || decl.params.iter().any(|p| {
                matches!(&p.ty, TypeExpr::Impl(bounds, false) if !bounds.iter().any(|b| matches!(b, Bound::Fn(..))))
                    || matches!(&p.ty, TypeExpr::Ref(_, inner) if matches!(&**inner, TypeExpr::Impl(bounds, false) if !bounds.iter().any(|b| matches!(b, Bound::Fn(..)))))
            })
    }

    /// Le type d'une contrainte de fermeture : `Fn(i64) -> bool`.
    pub(super) fn fn_bound(&mut self, bounds: &[Bound], line: u32) -> R<Option<Ty>> {
        for b in bounds {
            if let Bound::Fn(params, ret) = b {
                let params = params
                    .iter()
                    .map(|p| self.ty(p, line))
                    .collect::<R<Vec<_>>>()?;
                let ret = match ret {
                    Some(r) => self.ty(r, line)?,
                    None => Ty::Unit,
                };
                return Ok(Some(Ty::Fn(params, Box::new(ret))));
            }
        }
        Ok(None)
    }

    /// Appel d'une fonction générique : les types sont déduits des arguments, la copie
    /// correspondante sera fabriquée à la fin.
    pub(super) fn call_generic(
        &mut self,
        decl: &FnDecl,
        args: &[Expr],
        line: u32,
        out: &mut Vec<CStmt>,
    ) -> R<(CExpr, Ty)> {
        let mut named = Vec::new();
        let mut map = HashMap::new();
        for (g, bounds) in &decl.generics {
            let ty = match self.fn_bound(bounds, line)? {
                Some(t) => t,
                None => self.types.fresh(),
            };
            map.insert(g.clone(), ty.clone());
            named.push((g.clone(), ty, bounds.clone()));
        }
        let saved = mem::replace(&mut self.generics, map);
        let saved_anon = self.anonymous.replace(Vec::new());
        let slot = format!("fn {}#{}", decl.name, self.instances.len() + 1);
        let sig = self.signature(decl, slot.clone(), decl.name.clone());
        let anonymous = mem::replace(&mut self.anonymous, saved_anon).unwrap_or_default();
        self.generics = saved;
        let sig = sig?;
        let result = self.call_user(&sig, None, args, line, out)?;
        self.instances.push(Instance {
            slot,
            decl: decl.clone(),
            named,
            anonymous,
            line,
        });
        Ok(result)
    }

    fn satisfies(&mut self, ty: &Ty, bounds: &[Bound], what: &str, line: u32) -> R<()> {
        let ty = self.types.strip(ty);
        let shown = self.types.display(&ty);
        if matches!(ty, Ty::Var(_)) {
            return Err(rerr(
                line,
                "E0282",
                format!("impossible de deviner le type de « {what} » : précise-le"),
                format!("cannot infer the type of '{what}': specify it"),
            ));
        }
        for b in bounds {
            let Bound::Trait(t) = b else { continue };
            let ok = match t.as_str() {
                "Copy" => self.types.is_copy(&ty),
                "Clone" => self.types.is_clone(&ty),
                "Debug" => self.types.has_debug(&ty),
                "Display" => self.types.has_display(&ty),
                "PartialEq" | "Eq" => self.types.has_eq(&ty),
                "PartialOrd" | "Ord" => matches!(ty, Ty::Int | Ty::Str | Ty::String),
                _ => match ty {
                    Ty::Adt(i) => self.impls.contains(&(t.clone(), i)),
                    _ => false,
                },
            };
            if !ok {
                return Err(rerr(
                    line,
                    "E0277",
                    format!(
                        "« {shown} » n'implémente pas le trait « {t} » (demandé pour « {what} ») : ajoute impl {t} for {shown}"
                    ),
                    format!(
                        "'{shown}' does not implement the trait '{t}' (required for '{what}'): add impl {t} for {shown}"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Fabrique les copies des fonctions génériques (en boucle : une copie peut en appeler
    /// d'autres).
    pub(super) fn instantiate_all(&mut self) -> R<Vec<CStmt>> {
        let mut done: HashMap<String, String> = HashMap::new();
        let mut code = Vec::new();
        let mut next = 0;
        while next < self.instances.len() {
            let instance = self.instances[next].clone();
            next += 1;
            let mut key = instance.decl.name.clone();
            let mut map = HashMap::new();
            for (g, ty, bounds) in &instance.named {
                self.satisfies(ty, bounds, g, instance.line)?;
                let resolved = self.types.strip(ty);
                key.push_str(&format!("|{}", self.types.display(&resolved)));
                map.insert(g.clone(), resolved);
            }
            let mut anonymous = Vec::new();
            for (ty, bounds) in &instance.anonymous {
                self.satisfies(ty, bounds, "impl …", instance.line)?;
                let resolved = self.types.strip(ty);
                key.push_str(&format!("|{}", self.types.display(&resolved)));
                anonymous.push(resolved);
            }
            if let Some(existing) = done.get(&key) {
                code.push(CStmt {
                    line: instance.decl.line,
                    kind: K::Assign(Target::Name(instance.slot.clone()), name(existing)),
                });
                continue;
            }
            done.insert(key, instance.slot.clone());
            let saved = mem::replace(&mut self.generics, map);
            self.fill = Some(anonymous.into());
            self.self_ty = None;
            let sig = self.signature(
                &instance.decl,
                instance.slot.clone(),
                instance.decl.name.clone(),
            )?;
            let def = self.function(&instance.decl, &sig)?;
            self.fill = None;
            self.generics = saved;
            code.push(def);
        }
        Ok(code)
    }

    /// Type d'un `impl Trait` / `dyn Trait`.
    pub(super) fn impl_type(&mut self, bounds: &[Bound], dynamic: bool, line: u32) -> R<Ty> {
        if let Some(t) = self.fn_bound(bounds, line)? {
            return Ok(t);
        }
        if dynamic {
            return Err(unsupported(
                line,
                "dyn Trait (utilise un générique : fn f<T: Trait>(x: &T))",
                "dyn Trait (use a generic: fn f<T: Trait>(x: &T))",
            ));
        }
        if let Some(fill) = &mut self.fill
            && let Some(t) = fill.pop_front()
        {
            return Ok(t);
        }
        if let Some(anonymous) = &mut self.anonymous {
            let t = self.types.fresh();
            anonymous.push((t.clone(), bounds.to_vec()));
            return Ok(t);
        }
        Err(unsupported(
            line,
            "impl Trait ici (seulement pour un paramètre de fonction)",
            "impl Trait here (only for a function parameter)",
        ))
    }
}
