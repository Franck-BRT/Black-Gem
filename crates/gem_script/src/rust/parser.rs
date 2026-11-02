//! Analyse syntaxique du sous-ensemble Rust.

use super::{
    ast::*,
    lexer::{Tok, Token, tokenize},
};
use crate::error::{ErrorKind, ScriptError};

pub fn parse(source: &str) -> Result<Vec<Item>, ScriptError> {
    let tokens = tokenize(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut items = Vec::new();
    while parser.peek() != &Tok::Eof {
        if let Some(item) = parser.item()? {
            items.push(item);
        }
    }
    Ok(items)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

pub(super) fn unsupported(line: u32, french: &str, english: &str) -> ScriptError {
    ScriptError::new(
        line,
        ErrorKind::Rust {
            code: "",
            french: format!("{french} (pas encore dans le Rust du jeu)"),
            english: format!("{english} (not in the game's Rust yet)"),
        },
    )
}

/// Expressions qui se terminent par un bloc : pas besoin de « ; » après elles.
fn ends_with_block(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::If(..)
            | ExprKind::IfLet(..)
            | ExprKind::While(..)
            | ExprKind::WhileLet(..)
            | ExprKind::Loop(_)
            | ExprKind::For(..)
            | ExprKind::Match(..)
            | ExprKind::Block(_)
    )
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn peek_at(&self, offset: usize) -> &Tok {
        let i = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[i].tok
    }

    fn line(&self) -> u32 {
        self.tokens[self.pos].line
    }

    fn prev_line(&self) -> u32 {
        self.tokens[self.pos.saturating_sub(1)].line
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos].tok.clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn error(&self, kind: ErrorKind) -> ScriptError {
        ScriptError::new(self.line(), kind)
    }

    fn unexpected(&self) -> ScriptError {
        self.error(ErrorKind::UnexpectedToken(self.peek().describe()))
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn is_keyword(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Keyword(q) if *q == k)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if self.is_punct(p) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, k: &str) -> bool {
        if self.is_keyword(k) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, p: &'static str) -> Result<(), ScriptError> {
        if self.eat_punct(p) {
            Ok(())
        } else if p == ";" {
            // L'oubli le plus courant : le signaler sur la ligne de l'instruction.
            Err(ScriptError::new(
                self.prev_line(),
                ErrorKind::Rust {
                    code: "",
                    french: "il manque « ; » à la fin de l'instruction".into(),
                    english: "missing ';' at the end of the statement".into(),
                },
            ))
        } else {
            Err(self.error(ErrorKind::Expected(p)))
        }
    }

    fn ident(&mut self) -> Result<String, ScriptError> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error(ErrorKind::Expected("nom / name"))),
        }
    }

    // ----- Éléments du programme -----

    fn item(&mut self) -> Result<Option<Item>, ScriptError> {
        let mut derives = Vec::new();
        while self.is_punct("#") {
            self.attribute(&mut derives)?;
        }
        self.eat_keyword("pub");
        let line = self.line();
        match self.peek().clone() {
            Tok::Keyword("fn") => Ok(Some(Item::Fn(self.function(false)?))),
            Tok::Keyword("struct") => {
                self.advance();
                let name = self.ident()?;
                self.no_generics()?;
                let mut fields = Vec::new();
                if self.eat_punct(";") {
                    return Ok(Some(Item::Struct(StructDecl {
                        name,
                        line,
                        fields,
                        derives,
                    })));
                }
                if self.is_punct("(") {
                    return Err(unsupported(
                        line,
                        "les structures tuples « struct A(T); » : utilise des champs nommés",
                        "tuple structs 'struct A(T);': use named fields",
                    ));
                }
                self.expect_punct("{")?;
                while !self.eat_punct("}") {
                    self.eat_keyword("pub");
                    let field = self.ident()?;
                    self.expect_punct(":")?;
                    let ty = self.type_expr()?;
                    fields.push((field, ty));
                    if !self.eat_punct(",") {
                        self.expect_punct("}")?;
                        break;
                    }
                }
                Ok(Some(Item::Struct(StructDecl {
                    name,
                    line,
                    fields,
                    derives,
                })))
            }
            Tok::Keyword("enum") => {
                self.advance();
                let name = self.ident()?;
                self.no_generics()?;
                self.expect_punct("{")?;
                let mut variants = Vec::new();
                while !self.eat_punct("}") {
                    let variant = self.ident()?;
                    let mut payload = Vec::new();
                    if self.eat_punct("(") {
                        while !self.eat_punct(")") {
                            payload.push(self.type_expr()?);
                            if !self.eat_punct(",") {
                                self.expect_punct(")")?;
                                break;
                            }
                        }
                    } else if self.is_punct("{") {
                        return Err(unsupported(
                            self.line(),
                            "les variantes à champs nommés : utilise Variante(T1, T2)",
                            "variants with named fields: use Variant(T1, T2)",
                        ));
                    }
                    variants.push((variant, payload));
                    if !self.eat_punct(",") {
                        self.expect_punct("}")?;
                        break;
                    }
                }
                Ok(Some(Item::Enum(EnumDecl {
                    name,
                    line,
                    variants,
                    derives,
                })))
            }
            Tok::Keyword("impl") => {
                self.advance();
                self.no_generics()?;
                let type_name = self.ident()?;
                if self.is_keyword("for") {
                    return Err(unsupported(
                        line,
                        "les traits (impl Trait for Type)",
                        "traits (impl Trait for Type)",
                    ));
                }
                self.expect_punct("{")?;
                let mut methods = Vec::new();
                while !self.eat_punct("}") {
                    while self.is_punct("#") {
                        self.attribute(&mut Vec::new())?;
                    }
                    self.eat_keyword("pub");
                    if !self.is_keyword("fn") {
                        return Err(self.unexpected());
                    }
                    methods.push(self.function(true)?);
                }
                Ok(Some(Item::Impl {
                    type_name,
                    line,
                    methods,
                }))
            }
            Tok::Keyword("const") | Tok::Keyword("static") => {
                self.advance();
                if self.eat_keyword("mut") {
                    return Err(unsupported(
                        line,
                        "les variables globales modifiables (static mut)",
                        "mutable globals (static mut)",
                    ));
                }
                let name = self.ident()?;
                self.expect_punct(":")?;
                let ty = self.type_expr()?;
                self.expect_punct("=")?;
                let value = self.expr()?;
                self.expect_punct(";")?;
                Ok(Some(Item::Const {
                    name,
                    line,
                    ty,
                    value,
                }))
            }
            Tok::Keyword("use") => {
                // `use std::…;` : rien à importer dans le jeu.
                while !self.eat_punct(";") {
                    if self.peek() == &Tok::Eof {
                        return Err(self.error(ErrorKind::Expected(";")));
                    }
                    self.advance();
                }
                Ok(None)
            }
            Tok::Keyword("trait") => Err(unsupported(line, "les traits", "traits")),
            Tok::Keyword("mod") => Err(unsupported(line, "les modules", "modules")),
            Tok::Keyword("let") | Tok::Ident(_) | Tok::Macro(_) => Err(ScriptError::new(
                line,
                ErrorKind::Rust {
                    code: "",
                    french: "en Rust, les instructions vont dans une fonction : écris-les dans fn main() { … }".into(),
                    english: "in Rust, statements go inside a function: write them in fn main() { … }".into(),
                },
            )),
            _ => Err(self.unexpected()),
        }
    }

    /// `#[derive(Debug, Clone)]` (les autres attributs sont ignorés).
    fn attribute(&mut self, derives: &mut Vec<String>) -> Result<(), ScriptError> {
        self.expect_punct("#")?;
        self.eat_punct("!");
        self.expect_punct("[")?;
        let is_derive = matches!(self.peek(), Tok::Ident(name) if name == "derive");
        let mut depth = 1;
        while depth > 0 {
            match self.advance() {
                Tok::Punct("[") => depth += 1,
                Tok::Punct("]") => depth -= 1,
                Tok::Ident(name) if is_derive && name != "derive" => derives.push(name),
                Tok::Eof => return Err(self.error(ErrorKind::Expected("]"))),
                _ => {}
            }
        }
        Ok(())
    }

    fn no_generics(&mut self) -> Result<(), ScriptError> {
        if self.is_punct("<") {
            return Err(unsupported(
                self.line(),
                "les types génériques <T>",
                "generic types <T>",
            ));
        }
        Ok(())
    }

    fn function(&mut self, in_impl: bool) -> Result<FnDecl, ScriptError> {
        let line = self.line();
        self.advance(); // fn
        let name = self.ident()?;
        self.no_generics()?;
        self.expect_punct("(")?;
        let mut self_param = None;
        let mut params = Vec::new();
        let mut first = true;
        while !self.eat_punct(")") {
            // self, &self, &mut self, mut self
            let is_self = match (self.peek(), self.peek_at(1), self.peek_at(2)) {
                (Tok::Keyword("self"), _, _) => Some((SelfKind::Value, false, 1)),
                (Tok::Keyword("mut"), Tok::Keyword("self"), _) => Some((SelfKind::Value, true, 2)),
                (Tok::Punct("&"), Tok::Keyword("self"), _) => Some((SelfKind::Ref, false, 2)),
                (Tok::Punct("&"), Tok::Keyword("mut"), Tok::Keyword("self")) => {
                    Some((SelfKind::RefMut, false, 3))
                }
                _ => None,
            };
            if let Some((kind, mutable, len)) = is_self {
                if !first || !in_impl {
                    return Err(ScriptError::new(
                        self.line(),
                        ErrorKind::Rust {
                            code: "E0424",
                            french: "« self » n'existe que comme premier paramètre d'une méthode, dans un bloc impl".into(),
                            english: "'self' only exists as the first parameter of a method, in an impl block".into(),
                        },
                    ));
                }
                for _ in 0..len {
                    self.advance();
                }
                self_param = Some((kind, mutable));
            } else {
                let mutable = self.eat_keyword("mut");
                let param = self.ident()?;
                if !self.is_punct(":") {
                    return Err(ScriptError::new(
                        self.line(),
                        ErrorKind::Rust {
                            code: "",
                            french: format!(
                                "en Rust, chaque paramètre a un type : écris « {param}: i64 » (par exemple)"
                            ),
                            english: format!(
                                "in Rust, every parameter has a type: write '{param}: i64' (for example)"
                            ),
                        },
                    ));
                }
                self.advance();
                let ty = self.type_expr()?;
                params.push(Param {
                    name: param,
                    mutable,
                    ty,
                });
            }
            first = false;
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        let ret = if self.eat_punct("->") {
            Some(self.type_expr()?)
        } else {
            None
        };
        if self.is_keyword("where") {
            return Err(unsupported(
                self.line(),
                "les clauses where",
                "where clauses",
            ));
        }
        let body = self.block()?;
        Ok(FnDecl {
            name,
            line,
            self_param,
            params,
            ret,
            body,
        })
    }

    fn type_expr(&mut self) -> Result<TypeExpr, ScriptError> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Punct("&") => {
                self.advance();
                if matches!(self.peek(), Tok::Lifetime(_)) {
                    self.advance();
                }
                let mutable = self.eat_keyword("mut");
                Ok(TypeExpr::Ref(mutable, Box::new(self.type_expr()?)))
            }
            Tok::Punct("&&") => {
                self.advance();
                let inner = TypeExpr::Ref(false, Box::new(self.type_expr()?));
                Ok(TypeExpr::Ref(false, Box::new(inner)))
            }
            Tok::Punct("(") => {
                self.advance();
                let mut items = Vec::new();
                while !self.eat_punct(")") {
                    items.push(self.type_expr()?);
                    if !self.eat_punct(",") {
                        self.expect_punct(")")?;
                        break;
                    }
                }
                Ok(TypeExpr::Tuple(items))
            }
            Tok::Punct("[") => {
                self.advance();
                let inner = self.type_expr()?;
                if self.eat_punct(";") {
                    self.expr()?;
                }
                self.expect_punct("]")?;
                Ok(TypeExpr::Array(Box::new(inner)))
            }
            Tok::Ident(name) if name == "_" => {
                self.advance();
                Ok(TypeExpr::Infer)
            }
            Tok::Ident(_) | Tok::Keyword("Self") => {
                let mut name = match self.advance() {
                    Tok::Ident(name) => name,
                    _ => "Self".to_string(),
                };
                // std::collections::X → X
                while self.eat_punct("::") {
                    name = self.ident()?;
                }
                let mut args = Vec::new();
                if self.eat_punct("<") {
                    while !self.eat_punct(">") {
                        args.push(self.type_expr()?);
                        if !self.eat_punct(",") {
                            self.expect_punct(">")?;
                            break;
                        }
                    }
                }
                Ok(TypeExpr::Path(name, args))
            }
            Tok::Lifetime(_) => Err(unsupported(
                line,
                "les durées de vie dans les types ('a)",
                "lifetimes in types ('a)",
            )),
            Tok::Keyword("impl") | Tok::Keyword("dyn") => Err(unsupported(
                line,
                "les types impl Trait / dyn Trait",
                "impl Trait / dyn Trait types",
            )),
            Tok::Punct("!") => {
                self.advance();
                Ok(TypeExpr::Path("!".into(), Vec::new()))
            }
            _ => Err(self.error(ErrorKind::Expected("type"))),
        }
    }

    // ----- Blocs et instructions -----

    fn block(&mut self) -> Result<Block, ScriptError> {
        let line = self.line();
        self.expect_punct("{")?;
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            if self.is_punct("}") {
                let end_line = self.line();
                self.advance();
                return Ok(Block {
                    stmts,
                    tail,
                    line,
                    end_line,
                });
            }
            if let Some(expr) = tail.take() {
                // Une expression à bloc suivie d'autre chose : c'était une instruction.
                stmts.push(Stmt::Expr(*expr));
            }
            if self.peek() == &Tok::Eof {
                return Err(self.error(ErrorKind::Expected("}")));
            }
            if self.eat_punct(";") {
                continue;
            }
            if self.is_keyword("let") {
                stmts.push(self.let_stmt()?);
                continue;
            }
            if self.is_keyword("fn") || self.is_keyword("struct") || self.is_keyword("enum") {
                return Err(unsupported(
                    self.line(),
                    "les définitions dans une fonction : place-les à côté de fn main()",
                    "definitions inside a function: put them next to fn main()",
                ));
            }
            let expr = self.expr_statement()?;
            if self.eat_punct(";") {
                stmts.push(Stmt::Expr(expr));
            } else if self.is_punct("}") || ends_with_block(&expr.kind) {
                tail = Some(Box::new(expr));
            } else {
                return Err(ScriptError::new(
                    expr.line.max(self.prev_line()),
                    ErrorKind::Rust {
                        code: "",
                        french: "il manque « ; » à la fin de l'instruction".into(),
                        english: "missing ';' at the end of the statement".into(),
                    },
                ));
            }
        }
    }

    fn let_stmt(&mut self) -> Result<Stmt, ScriptError> {
        let line = self.line();
        self.advance(); // let
        let pat = self.pattern()?;
        let ty = if self.eat_punct(":") {
            Some(self.type_expr()?)
        } else {
            None
        };
        let init = if self.eat_punct("=") {
            Some(self.expr()?)
        } else {
            None
        };
        let otherwise = if init.is_some() && self.eat_keyword("else") {
            Some(self.block()?)
        } else {
            None
        };
        self.expect_punct(";")?;
        Ok(Stmt::Let {
            pat,
            ty,
            init,
            otherwise,
            line,
        })
    }

    /// Une instruction-expression : un `if`/`match`/`loop`… en tête n'est pas suivi d'opérateurs.
    fn expr_statement(&mut self) -> Result<Expr, ScriptError> {
        if matches!(
            self.peek(),
            Tok::Keyword("if" | "match" | "loop" | "while" | "for") | Tok::Punct("{")
        ) {
            let expr = self.primary(false)?;
            if self.is_punct(".") || self.is_punct("?") {
                // `match x { … }.unwrap()` : on continue comme une expression.
                let expr = self.postfix_from(expr)?;
                return self.binary_rest(expr, 0, false);
            }
            return Ok(expr);
        }
        self.expr()
    }

    // ----- Expressions -----

    pub fn expr(&mut self) -> Result<Expr, ScriptError> {
        self.expr_with(false)
    }

    /// `no_struct` : dans une condition (`if x == A { … }`), `{` ouvre le bloc et pas une
    /// structure.
    fn expr_with(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let line = self.line();
        let left = self.range(no_struct)?;
        let op = match self.peek() {
            Tok::Punct("=") => None,
            Tok::Punct("+=") => Some(BinOp::Add),
            Tok::Punct("-=") => Some(BinOp::Sub),
            Tok::Punct("*=") => Some(BinOp::Mul),
            Tok::Punct("/=") => Some(BinOp::Div),
            Tok::Punct("%=") => Some(BinOp::Rem),
            _ => return Ok(left),
        };
        self.advance();
        let right = self.expr_with(no_struct)?;
        let kind = match op {
            None => ExprKind::Assign(Box::new(left), Box::new(right)),
            Some(op) => ExprKind::CompoundAssign(op, Box::new(left), Box::new(right)),
        };
        Ok(Expr { kind, line })
    }

    fn range(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let line = self.line();
        let start = if self.is_punct("..") || self.is_punct("..=") {
            None
        } else {
            Some(self.binary(0, no_struct)?)
        };
        let inclusive = if self.eat_punct("..") {
            false
        } else if self.eat_punct("..=") {
            true
        } else {
            return Ok(start.expect("expression"));
        };
        let has_end = !matches!(
            self.peek(),
            Tok::Punct(")" | "]" | "}" | ";" | ",") | Tok::Eof
        ) && !(no_struct && self.is_punct("{"));
        let end = if has_end {
            Some(Box::new(self.binary(0, no_struct)?))
        } else {
            None
        };
        Ok(Expr {
            kind: ExprKind::Range(start.map(Box::new), end, inclusive),
            line,
        })
    }

    fn binary_op(&self) -> Option<(BinOp, u8)> {
        let op = match self.peek() {
            Tok::Punct("||") => (BinOp::Or, 1),
            Tok::Punct("&&") => (BinOp::And, 2),
            Tok::Punct("==") => (BinOp::Eq, 3),
            Tok::Punct("!=") => (BinOp::Ne, 3),
            Tok::Punct("<") => (BinOp::Lt, 3),
            Tok::Punct("<=") => (BinOp::Le, 3),
            Tok::Punct(">") => (BinOp::Gt, 3),
            Tok::Punct(">=") => (BinOp::Ge, 3),
            Tok::Punct("+") => (BinOp::Add, 4),
            Tok::Punct("-") => (BinOp::Sub, 4),
            Tok::Punct("*") => (BinOp::Mul, 5),
            Tok::Punct("/") => (BinOp::Div, 5),
            Tok::Punct("%") => (BinOp::Rem, 5),
            _ => return None,
        };
        Some(op)
    }

    fn binary(&mut self, min: u8, no_struct: bool) -> Result<Expr, ScriptError> {
        let left = self.cast(no_struct)?;
        self.binary_rest(left, min, no_struct)
    }

    fn binary_rest(
        &mut self,
        mut left: Expr,
        min: u8,
        no_struct: bool,
    ) -> Result<Expr, ScriptError> {
        while let Some((op, prec)) = self.binary_op() {
            if prec < min {
                break;
            }
            let line = self.line();
            self.advance();
            let right = self.binary(prec + 1, no_struct)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                line,
            };
            if prec == 3 && matches!(self.binary_op(), Some((_, 3))) {
                return Err(ScriptError::new(
                    self.line(),
                    ErrorKind::Rust {
                        code: "",
                        french:
                            "les comparaisons ne s'enchaînent pas en Rust : écris a < b && b < c"
                                .into(),
                        english: "comparisons cannot be chained in Rust: write a < b && b < c"
                            .into(),
                    },
                ));
            }
        }
        if self.is_punct("|") || self.is_punct("&") {
            return Err(unsupported(
                self.line(),
                "les opérateurs bit à bit (|, &, ^)",
                "bitwise operators (|, &, ^)",
            ));
        }
        Ok(left)
    }

    fn cast(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let mut expr = self.unary(no_struct)?;
        while self.is_keyword("as") {
            let line = self.line();
            self.advance();
            let ty = self.type_expr()?;
            expr = Expr {
                kind: ExprKind::Cast(Box::new(expr), ty),
                line,
            };
        }
        Ok(expr)
    }

    fn unary(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let line = self.line();
        let kind = match self.peek() {
            Tok::Punct("-") => {
                self.advance();
                ExprKind::Unary(UnOp::Neg, Box::new(self.unary(no_struct)?))
            }
            Tok::Punct("!") => {
                self.advance();
                ExprKind::Unary(UnOp::Not, Box::new(self.unary(no_struct)?))
            }
            Tok::Punct("*") => {
                self.advance();
                ExprKind::Unary(UnOp::Deref, Box::new(self.unary(no_struct)?))
            }
            Tok::Punct("&") => {
                self.advance();
                let mutable = self.eat_keyword("mut");
                ExprKind::Ref(mutable, Box::new(self.unary(no_struct)?))
            }
            Tok::Punct("&&") => {
                self.advance();
                let inner = Expr {
                    kind: ExprKind::Ref(false, Box::new(self.unary(no_struct)?)),
                    line,
                };
                ExprKind::Ref(false, Box::new(inner))
            }
            _ => return self.postfix(no_struct),
        };
        Ok(Expr { kind, line })
    }

    fn postfix(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let expr = self.primary(no_struct)?;
        self.postfix_from(expr)
    }

    fn postfix_from(&mut self, mut expr: Expr) -> Result<Expr, ScriptError> {
        loop {
            let line = self.line();
            if self.eat_punct("?") {
                expr = Expr {
                    kind: ExprKind::Try(Box::new(expr)),
                    line,
                };
            } else if self.eat_punct(".") {
                match self.advance() {
                    Tok::Int(n) => {
                        expr = Expr {
                            kind: ExprKind::Field(Box::new(expr), n.to_string()),
                            line,
                        }
                    }
                    Tok::Ident(name) => {
                        if self.eat_punct("::") {
                            self.turbofish()?;
                        }
                        if self.is_punct("(") {
                            let args = self.call_args()?;
                            expr = Expr {
                                kind: ExprKind::MethodCall(Box::new(expr), name, args),
                                line,
                            };
                        } else {
                            expr = Expr {
                                kind: ExprKind::Field(Box::new(expr), name),
                                line,
                            };
                        }
                    }
                    _ => {
                        self.pos -= 1;
                        return Err(self.error(ErrorKind::Expected("nom / name")));
                    }
                }
            } else if self.is_punct("(") {
                let args = self.call_args()?;
                expr = Expr {
                    kind: ExprKind::Call(Box::new(expr), args),
                    line,
                };
            } else if self.eat_punct("[") {
                let index = self.expr()?;
                self.expect_punct("]")?;
                expr = Expr {
                    kind: ExprKind::Index(Box::new(expr), Box::new(index)),
                    line,
                };
            } else {
                return Ok(expr);
            }
        }
    }

    /// `::<T>` (le type est ignoré : il est déduit).
    fn turbofish(&mut self) -> Result<(), ScriptError> {
        self.expect_punct("<")?;
        while !self.eat_punct(">") {
            self.type_expr()?;
            if !self.eat_punct(",") {
                self.expect_punct(">")?;
                break;
            }
        }
        Ok(())
    }

    fn call_args(&mut self) -> Result<Vec<Expr>, ScriptError> {
        self.expect_punct("(")?;
        let mut args = Vec::new();
        while !self.eat_punct(")") {
            args.push(self.expr()?);
            if !self.eat_punct(",") {
                self.expect_punct(")")?;
                break;
            }
        }
        Ok(args)
    }

    fn primary(&mut self, no_struct: bool) -> Result<Expr, ScriptError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            Tok::Int(n) => {
                self.advance();
                ExprKind::Int(n)
            }
            Tok::Str(s) => {
                self.advance();
                ExprKind::Str(s)
            }
            Tok::Keyword("true") => {
                self.advance();
                ExprKind::Bool(true)
            }
            Tok::Keyword("false") => {
                self.advance();
                ExprKind::Bool(false)
            }
            Tok::Ident(_) | Tok::Keyword("self") | Tok::Keyword("Self") => {
                let mut path = Vec::new();
                loop {
                    match self.advance() {
                        Tok::Ident(name) => path.push(name),
                        Tok::Keyword(k) if k == "self" || k == "Self" => path.push(k.to_string()),
                        _ => {
                            self.pos -= 1;
                            return Err(self.error(ErrorKind::Expected("nom / name")));
                        }
                    }
                    if !self.is_punct("::") {
                        break;
                    }
                    self.advance();
                    if self.is_punct("<") {
                        self.turbofish()?;
                        if !self.eat_punct("::") {
                            break;
                        }
                    }
                }
                let is_type = path
                    .last()
                    .is_some_and(|name| name.chars().next().is_some_and(char::is_uppercase));
                if self.is_punct("{") && !no_struct && is_type {
                    return self.struct_literal(path, line);
                }
                ExprKind::Path(path)
            }
            Tok::Punct("(") => {
                self.advance();
                let mut items = Vec::new();
                let mut trailing_comma = false;
                while !self.eat_punct(")") {
                    items.push(self.expr()?);
                    trailing_comma = self.eat_punct(",");
                    if !trailing_comma {
                        self.expect_punct(")")?;
                        break;
                    }
                }
                if items.len() == 1 && !trailing_comma {
                    return Ok(items.pop().unwrap());
                }
                ExprKind::Tuple(items)
            }
            Tok::Punct("[") => {
                self.advance();
                self.array_items("]")?
            }
            Tok::Punct("{") => ExprKind::Block(self.block()?),
            Tok::Keyword("if") => return self.if_expr(),
            Tok::Keyword("match") => {
                self.advance();
                let scrutinee = self.expr_with(true)?;
                self.expect_punct("{")?;
                let mut arms = Vec::new();
                while !self.eat_punct("}") {
                    let arm_line = self.line();
                    let pat = self.pattern()?;
                    let guard = if self.eat_keyword("if") {
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    self.expect_punct("=>")?;
                    let body = self.expr_statement_or_expr()?;
                    let block_like = ends_with_block(&body.kind);
                    arms.push(Arm {
                        pat,
                        guard,
                        body,
                        line: arm_line,
                    });
                    if !self.eat_punct(",") && !block_like {
                        self.expect_punct("}")?;
                        break;
                    }
                }
                ExprKind::Match(Box::new(scrutinee), arms)
            }
            Tok::Keyword("loop") => {
                self.advance();
                ExprKind::Loop(self.block()?)
            }
            Tok::Keyword("while") => {
                self.advance();
                if self.eat_keyword("let") {
                    let pat = self.pattern()?;
                    self.expect_punct("=")?;
                    let value = self.expr_with(true)?;
                    ExprKind::WhileLet(pat, Box::new(value), self.block()?)
                } else {
                    let condition = self.expr_with(true)?;
                    ExprKind::While(Box::new(condition), self.block()?)
                }
            }
            Tok::Keyword("for") => {
                self.advance();
                let pat = self.pattern()?;
                if !self.eat_keyword("in") {
                    return Err(self.error(ErrorKind::Expected("in")));
                }
                let iterable = self.expr_with(true)?;
                ExprKind::For(pat, Box::new(iterable), self.block()?)
            }
            Tok::Keyword("return") => {
                self.advance();
                ExprKind::Return(self.optional_value()?)
            }
            Tok::Keyword("break") => {
                self.advance();
                ExprKind::Break(self.optional_value()?)
            }
            Tok::Keyword("continue") => {
                self.advance();
                ExprKind::Continue
            }
            Tok::Macro(name) => {
                self.advance();
                return self.macro_call(&name, line);
            }
            Tok::Lifetime(_) => {
                return Err(unsupported(
                    line,
                    "les étiquettes de boucle ('externe: loop)",
                    "loop labels ('outer: loop)",
                ));
            }
            Tok::Punct("|") | Tok::Punct("||") | Tok::Keyword("move") => {
                return Err(unsupported(line, "les fermetures |x| …", "closures |x| …"));
            }
            _ => return Err(self.unexpected()),
        };
        Ok(Expr { kind, line })
    }

    /// Le corps d'une branche de `match` : une expression (avec ses opérateurs).
    fn expr_statement_or_expr(&mut self) -> Result<Expr, ScriptError> {
        if self.is_punct("{") {
            let line = self.line();
            let block = self.block()?;
            return Ok(Expr {
                kind: ExprKind::Block(block),
                line,
            });
        }
        self.expr()
    }

    fn optional_value(&mut self) -> Result<Option<Box<Expr>>, ScriptError> {
        if matches!(self.peek(), Tok::Punct(";" | "}" | ",") | Tok::Eof) {
            Ok(None)
        } else {
            Ok(Some(Box::new(self.expr()?)))
        }
    }

    fn if_expr(&mut self) -> Result<Expr, ScriptError> {
        let line = self.line();
        self.advance(); // if
        let kind = if self.eat_keyword("let") {
            let pat = self.pattern()?;
            self.expect_punct("=")?;
            let value = self.expr_with(true)?;
            let then = self.block()?;
            ExprKind::IfLet(pat, Box::new(value), then, self.else_branch()?)
        } else {
            let condition = self.expr_with(true)?;
            let then = self.block()?;
            ExprKind::If(Box::new(condition), then, self.else_branch()?)
        };
        Ok(Expr { kind, line })
    }

    fn else_branch(&mut self) -> Result<Option<Box<Expr>>, ScriptError> {
        if !self.eat_keyword("else") {
            return Ok(None);
        }
        if self.is_keyword("if") {
            return Ok(Some(Box::new(self.if_expr()?)));
        }
        let line = self.line();
        let block = self.block()?;
        Ok(Some(Box::new(Expr {
            kind: ExprKind::Block(block),
            line,
        })))
    }

    fn struct_literal(&mut self, path: Vec<String>, line: u32) -> Result<Expr, ScriptError> {
        self.expect_punct("{")?;
        let mut fields = Vec::new();
        while !self.eat_punct("}") {
            if self.is_punct("..") {
                return Err(unsupported(
                    self.line(),
                    "la syntaxe ..autre dans une structure",
                    "the ..other syntax in a struct literal",
                ));
            }
            let field_line = self.line();
            let name = self.ident()?;
            let value = if self.eat_punct(":") {
                self.expr()?
            } else {
                // Raccourci : `Point { x, y }`.
                Expr {
                    kind: ExprKind::Path(vec![name.clone()]),
                    line: field_line,
                }
            };
            fields.push((name, value));
            if !self.eat_punct(",") {
                self.expect_punct("}")?;
                break;
            }
        }
        Ok(Expr {
            kind: ExprKind::StructLit(path, fields),
            line,
        })
    }

    fn array_items(&mut self, close: &'static str) -> Result<ExprKind, ScriptError> {
        let mut items = Vec::new();
        while !self.eat_punct(close) {
            let item = self.expr()?;
            if items.is_empty() && self.eat_punct(";") {
                let count = self.expr()?;
                self.expect_punct(close)?;
                return Ok(ExprKind::ArrayRepeat(Box::new(item), Box::new(count)));
            }
            items.push(item);
            if !self.eat_punct(",") {
                self.expect_punct(close)?;
                break;
            }
        }
        Ok(ExprKind::Array(items))
    }

    fn macro_call(&mut self, name: &str, line: u32) -> Result<Expr, ScriptError> {
        let (_, close) = match self.peek() {
            Tok::Punct("(") => ("(", ")"),
            Tok::Punct("[") => ("[", "]"),
            Tok::Punct("{") => ("{", "}"),
            _ => return Err(self.error(ErrorKind::Expected("("))),
        };
        self.advance();
        let format_macro = match name {
            "println" | "eprintln" => Some(FmtMacro::Println),
            "print" | "eprint" => Some(FmtMacro::Print),
            "format" => Some(FmtMacro::Format),
            "panic" | "unreachable" | "todo" | "unimplemented" => Some(FmtMacro::Panic),
            _ => None,
        };
        let kind = if let Some(kind) = format_macro {
            let template = match self.peek().clone() {
                Tok::Str(s) => {
                    self.advance();
                    s
                }
                Tok::Punct(p) if p == close => match name {
                    "unreachable" => "code inatteignable / unreachable code".into(),
                    "todo" | "unimplemented" => "pas encore écrit / not yet written".into(),
                    _ => String::new(),
                },
                _ => {
                    return Err(ScriptError::new(
                        line,
                        ErrorKind::Rust {
                            code: "",
                            french: format!(
                                "{name}! attend d'abord un texte entre guillemets, comme {name}!(\"{{}}\", x)"
                            ),
                            english: format!(
                                "{name}! first expects a string literal, like {name}!(\"{{}}\", x)"
                            ),
                        },
                    ));
                }
            };
            let mut args = Vec::new();
            while self.eat_punct(",") {
                if self.is_punct(close) {
                    break;
                }
                args.push(self.expr()?);
            }
            self.expect_punct(close)?;
            ExprKind::Format(kind, template, args)
        } else {
            match name {
                "vec" => self.array_items(close)?,
                "assert" => {
                    let condition = self.expr()?;
                    let message = if self.eat_punct(",") && !self.is_punct(close) {
                        let template = match self.advance() {
                            Tok::Str(s) => s,
                            _ => return Err(self.error(ErrorKind::Expected("\"…\""))),
                        };
                        let mut args = Vec::new();
                        while self.eat_punct(",") {
                            if self.is_punct(close) {
                                break;
                            }
                            args.push(self.expr()?);
                        }
                        Some(Box::new(Expr {
                            kind: ExprKind::Format(FmtMacro::Format, template, args),
                            line,
                        }))
                    } else {
                        None
                    };
                    self.expect_punct(close)?;
                    ExprKind::Assert(Box::new(condition), message)
                }
                "assert_eq" | "assert_ne" => {
                    let left = self.expr()?;
                    self.expect_punct(",")?;
                    let right = self.expr()?;
                    // Message éventuel : ignoré.
                    let mut depth = 0;
                    while depth > 0 || !self.is_punct(close) {
                        match self.advance() {
                            Tok::Punct("(" | "[" | "{") => depth += 1,
                            Tok::Punct(")" | "]" | "}") => depth -= 1,
                            Tok::Eof => return Err(self.error(ErrorKind::Expected(close))),
                            _ => {}
                        }
                    }
                    self.expect_punct(close)?;
                    ExprKind::AssertEq(Box::new(left), Box::new(right), name == "assert_eq")
                }
                _ => {
                    return Err(unsupported(
                        line,
                        &format!("la macro {name}!"),
                        &format!("the {name}! macro"),
                    ));
                }
            }
        };
        Ok(Expr { kind, line })
    }

    // ----- Motifs -----

    fn pattern(&mut self) -> Result<Pat, ScriptError> {
        self.eat_punct("|");
        let first = self.single_pattern()?;
        if !self.is_punct("|") {
            return Ok(first);
        }
        let mut options = vec![first];
        while self.eat_punct("|") {
            options.push(self.single_pattern()?);
        }
        Ok(Pat::Or(options))
    }

    fn pattern_int(&mut self) -> Result<Option<i64>, ScriptError> {
        let negative = self.is_punct("-") && matches!(self.peek_at(1), Tok::Int(_));
        if negative {
            self.advance();
        }
        match self.peek() {
            Tok::Int(n) => {
                let n = if negative { -*n } else { *n };
                self.advance();
                Ok(Some(n))
            }
            _ => Ok(None),
        }
    }

    fn single_pattern(&mut self) -> Result<Pat, ScriptError> {
        if let Some(n) = self.pattern_int()? {
            let inclusive = if self.eat_punct("..=") {
                true
            } else if self.eat_punct("..") {
                false
            } else {
                return Ok(Pat::Int(n));
            };
            let Some(end) = self.pattern_int()? else {
                return Err(self.error(ErrorKind::Expected("nombre / number")));
            };
            return Ok(Pat::Range(n, end, inclusive));
        }
        match self.peek().clone() {
            Tok::Str(s) => {
                self.advance();
                Ok(Pat::Str(s))
            }
            Tok::Keyword("true") => {
                self.advance();
                Ok(Pat::Bool(true))
            }
            Tok::Keyword("false") => {
                self.advance();
                Ok(Pat::Bool(false))
            }
            Tok::Punct("&") => {
                self.advance();
                self.eat_keyword("mut");
                Ok(Pat::Ref(Box::new(self.single_pattern()?)))
            }
            Tok::Punct("(") => {
                self.advance();
                let mut items = Vec::new();
                let mut trailing_comma = false;
                while !self.eat_punct(")") {
                    items.push(self.pattern()?);
                    trailing_comma = self.eat_punct(",");
                    if !trailing_comma {
                        self.expect_punct(")")?;
                        break;
                    }
                }
                if items.len() == 1 && !trailing_comma {
                    return Ok(items.pop().unwrap());
                }
                Ok(Pat::Tuple(items))
            }
            Tok::Keyword("ref") | Tok::Keyword("mut") => {
                let by_ref = self.eat_keyword("ref");
                let mutable = self.eat_keyword("mut");
                let name = self.ident()?;
                Ok(Pat::Bind {
                    name,
                    mutable,
                    by_ref,
                })
            }
            Tok::Ident(name) if name == "_" => {
                self.advance();
                Ok(Pat::Wild)
            }
            Tok::Ident(_) | Tok::Keyword("Self") => {
                let mut path = Vec::new();
                loop {
                    match self.advance() {
                        Tok::Ident(name) => path.push(name),
                        _ => path.push("Self".into()),
                    }
                    if !self.eat_punct("::") {
                        break;
                    }
                }
                if self.eat_punct("(") {
                    let mut items = Vec::new();
                    while !self.eat_punct(")") {
                        items.push(self.pattern()?);
                        if !self.eat_punct(",") {
                            self.expect_punct(")")?;
                            break;
                        }
                    }
                    return Ok(Pat::TupleStruct(path, items));
                }
                if self.is_punct("{") {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.eat_punct("}") {
                        if self.eat_punct("..") {
                            self.expect_punct("}")?;
                            break;
                        }
                        let field = self.ident()?;
                        let pat = if self.eat_punct(":") {
                            self.pattern()?
                        } else {
                            Pat::Bind {
                                name: field.clone(),
                                mutable: false,
                                by_ref: false,
                            }
                        };
                        fields.push((field, pat));
                        if !self.eat_punct(",") {
                            self.expect_punct("}")?;
                            break;
                        }
                    }
                    return Ok(Pat::Struct(path, fields));
                }
                if path.len() == 1 && path[0] != "None" && path[0] != "Self" {
                    return Ok(Pat::Bind {
                        name: path.pop().unwrap(),
                        mutable: false,
                        by_ref: false,
                    });
                }
                Ok(Pat::Path(path))
            }
            _ => Err(self.error(ErrorKind::Expected("motif / pattern"))),
        }
    }
}
