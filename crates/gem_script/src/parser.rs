//! Analyse syntaxique du sous-ensemble de Python reconnu par le jeu.

use crate::{
    ast::{BinOp, Expr, Stmt, StmtKind, Target},
    error::{ErrorKind, ScriptError},
    lexer::{Tok, Token, tokenize},
};

pub fn parse(source: &str) -> Result<Vec<Stmt>> {
    let tokens = tokenize(source)?;
    let mut parser = Parser { tokens, pos: 0 };
    let mut program = Vec::new();
    while parser.peek() != &Tok::Eof {
        if parser.peek() == &Tok::Indent {
            return Err(parser.error(ErrorKind::UnexpectedIndent));
        }
        program.push(parser.statement()?);
    }
    Ok(program)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

type Result<T> = std::result::Result<T, ScriptError>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn line(&self) -> u32 {
        self.tokens[self.pos].line
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos].tok.clone();
        if self.pos + 1 < self.tokens.len() {
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

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }

    fn is_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek(), Tok::Keyword(k) if *k == keyword)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        let found = self.is_op(op);
        if found {
            self.advance();
        }
        found
    }

    fn expect_op(&mut self, op: &'static str) -> Result<()> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.error(ErrorKind::Expected(op)))
        }
    }

    fn expect_name(&mut self) -> Result<String> {
        match self.advance() {
            Tok::Name(name) => Ok(name),
            _ => {
                self.pos -= 1;
                Err(self.error(ErrorKind::Expected("nom / name")))
            }
        }
    }

    fn end_of_statement(&mut self) -> Result<()> {
        match self.peek() {
            Tok::Newline => {
                self.advance();
                Ok(())
            }
            Tok::Eof | Tok::Dedent => Ok(()),
            _ => Err(self.unexpected()),
        }
    }

    /// Bloc après « : » : fin de ligne, indentation, instructions, fin d'indentation.
    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect_op(":")?;
        if self.peek() != &Tok::Newline {
            // Forme courte sur une ligne : `if x: y = 1`.
            let line = self.line();
            let kind = self.simple_statement()?;
            self.end_of_statement()?;
            return Ok(vec![Stmt { line, kind }]);
        }
        self.advance();
        if self.peek() != &Tok::Indent {
            return Err(self.error(ErrorKind::ExpectedIndent));
        }
        self.advance();
        let mut body = Vec::new();
        while !matches!(self.peek(), Tok::Dedent | Tok::Eof) {
            body.push(self.statement()?);
        }
        if self.peek() == &Tok::Dedent {
            self.advance();
        }
        Ok(body)
    }

    fn statement(&mut self) -> Result<Stmt> {
        let line = self.line();
        let kind = match self.peek() {
            Tok::Keyword("if") => {
                self.advance();
                let mut branches = vec![(self.expression()?, self.block()?)];
                let mut otherwise = Vec::new();
                loop {
                    if self.is_keyword("elif") {
                        self.advance();
                        branches.push((self.expression()?, self.block()?));
                    } else if self.is_keyword("else") {
                        self.advance();
                        otherwise = self.block()?;
                        break;
                    } else {
                        break;
                    }
                }
                StmtKind::If(branches, otherwise)
            }
            Tok::Keyword("while") => {
                self.advance();
                StmtKind::While(self.expression()?, self.block()?)
            }
            Tok::Keyword("for") => {
                self.advance();
                let target = self.for_target()?;
                if !self.is_keyword("in") {
                    return Err(self.error(ErrorKind::Expected("in")));
                }
                self.advance();
                StmtKind::For(target, self.expression()?, self.block()?)
            }
            Tok::Keyword("def") => {
                self.advance();
                let name = self.expect_name()?;
                self.expect_op("(")?;
                let mut params: Vec<String> = Vec::new();
                while !self.is_op(")") {
                    let param = self.expect_name()?;
                    if params.contains(&param) {
                        return Err(self.error(ErrorKind::DuplicateParameter(param)));
                    }
                    params.push(param);
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_op(")")?;
                StmtKind::Def(name, params, self.block()?)
            }
            Tok::Keyword("elif" | "else") => return Err(self.unexpected()),
            _ => {
                let kind = self.simple_statement()?;
                self.end_of_statement()?;
                kind
            }
        };
        Ok(Stmt { line, kind })
    }

    fn for_target(&mut self) -> Result<Target> {
        let parenthesized = self.eat_op("(");
        let mut names = vec![self.expect_name()?];
        while self.eat_op(",") {
            names.push(self.expect_name()?);
        }
        if parenthesized {
            self.expect_op(")")?;
        }
        Ok(if names.len() == 1 && !parenthesized {
            Target::Name(names.remove(0))
        } else {
            Target::Unpack(names)
        })
    }

    fn simple_statement(&mut self) -> Result<StmtKind> {
        match self.peek() {
            Tok::Keyword("pass") => {
                self.advance();
                return Ok(StmtKind::Pass);
            }
            Tok::Keyword("break") => {
                self.advance();
                return Ok(StmtKind::Break);
            }
            Tok::Keyword("continue") => {
                self.advance();
                return Ok(StmtKind::Continue);
            }
            Tok::Keyword("return") => {
                self.advance();
                if matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::Dedent) {
                    return Ok(StmtKind::Return(None));
                }
                return Ok(StmtKind::Return(Some(self.expression_list()?)));
            }
            Tok::Keyword("global") => {
                self.advance();
                let mut names = vec![self.expect_name()?];
                while self.eat_op(",") {
                    names.push(self.expect_name()?);
                }
                return Ok(StmtKind::Global(names));
            }
            _ => {}
        }

        let expr = self.expression_list()?;
        if self.eat_op("=") {
            let target = to_target(&expr).ok_or_else(|| self.error(ErrorKind::InvalidTarget))?;
            return Ok(StmtKind::Assign(target, self.expression_list()?));
        }
        let augmented = [
            ("+=", BinOp::Add),
            ("-=", BinOp::Sub),
            ("*=", BinOp::Mul),
            ("//=", BinOp::FloorDiv),
            ("%=", BinOp::Mod),
        ];
        for (op, bin) in augmented {
            if self.eat_op(op) {
                let Expr::Name(name) = expr else {
                    return Err(self.error(ErrorKind::InvalidTarget));
                };
                return Ok(StmtKind::AugAssign(name, bin, self.expression()?));
            }
        }
        if self.is_op("/") {
            return Err(self.error(ErrorKind::FloatDivision));
        }
        Ok(StmtKind::Expr(expr))
    }

    /// Une ou plusieurs expressions séparées par des virgules (tuple sans parenthèses).
    fn expression_list(&mut self) -> Result<Expr> {
        let first = self.expression()?;
        if !self.is_op(",") {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat_op(",") {
            if matches!(self.peek(), Tok::Newline | Tok::Eof) || self.is_op("=") {
                break;
            }
            items.push(self.expression()?);
        }
        Ok(Expr::Tuple(items))
    }

    fn expression(&mut self) -> Result<Expr> {
        let mut left = self.and_expr()?;
        while self.is_keyword("or") {
            self.advance();
            left = Expr::Or(Box::new(left), Box::new(self.and_expr()?));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut left = self.not_expr()?;
        while self.is_keyword("and") {
            self.advance();
            left = Expr::And(Box::new(left), Box::new(self.not_expr()?));
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> Result<Expr> {
        if self.is_keyword("not") {
            self.advance();
            return Ok(Expr::Not(Box::new(self.not_expr()?)));
        }
        self.comparison()
    }

    /// Comparaisons, y compris enchaînées : `a < b < c` devient `a < b and b < c`.
    fn comparison(&mut self) -> Result<Expr> {
        let first = self.arith()?;
        let mut parts = Vec::new();
        let mut previous = first.clone();
        loop {
            let op = match self.peek() {
                Tok::Op("==") => BinOp::Eq,
                Tok::Op("!=") => BinOp::Ne,
                Tok::Op("<") => BinOp::Lt,
                Tok::Op("<=") => BinOp::Le,
                Tok::Op(">") => BinOp::Gt,
                Tok::Op(">=") => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.arith()?;
            parts.push(Expr::Binary(
                op,
                Box::new(previous),
                Box::new(right.clone()),
            ));
            previous = right;
        }
        Ok(parts
            .into_iter()
            .reduce(|a, b| Expr::And(Box::new(a), Box::new(b)))
            .unwrap_or(first))
    }

    fn arith(&mut self) -> Result<Expr> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Tok::Op("+") => BinOp::Add,
                Tok::Op("-") => BinOp::Sub,
                _ => return Ok(left),
            };
            self.advance();
            left = Expr::Binary(op, Box::new(left), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Result<Expr> {
        let mut left = self.factor()?;
        loop {
            let op = match self.peek() {
                Tok::Op("*") => BinOp::Mul,
                Tok::Op("//") => BinOp::FloorDiv,
                Tok::Op("%") => BinOp::Mod,
                Tok::Op("/") => return Err(self.error(ErrorKind::FloatDivision)),
                _ => return Ok(left),
            };
            self.advance();
            left = Expr::Binary(op, Box::new(left), Box::new(self.factor()?));
        }
    }

    fn factor(&mut self) -> Result<Expr> {
        if self.eat_op("-") {
            return Ok(Expr::Neg(Box::new(self.factor()?)));
        }
        if self.eat_op("+") {
            return self.factor();
        }
        let base = self.postfix()?;
        if self.eat_op("**") {
            return Ok(Expr::Binary(
                BinOp::Pow,
                Box::new(base),
                Box::new(self.factor()?),
            ));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut expr = self.atom()?;
        loop {
            if self.eat_op("(") {
                let args = self.arguments(")")?;
                expr = Expr::Call(Box::new(expr), args);
            } else if self.eat_op("[") {
                let index = self.expression()?;
                self.expect_op("]")?;
                expr = Expr::Index(Box::new(expr), Box::new(index));
            } else if self.eat_op(".") {
                let method = self.expect_name()?;
                let Expr::Name(object) = expr else {
                    return Err(self.error(ErrorKind::UnsupportedMethod(method)));
                };
                self.expect_op("(")?;
                let args = self.arguments(")")?;
                expr = Expr::Method(object, method, args);
            } else {
                return Ok(expr);
            }
        }
    }

    fn arguments(&mut self, close: &'static str) -> Result<Vec<Expr>> {
        let mut args = Vec::new();
        while !self.is_op(close) {
            args.push(self.expression()?);
            if !self.eat_op(",") {
                break;
            }
        }
        self.expect_op(close)?;
        Ok(args)
    }

    fn atom(&mut self) -> Result<Expr> {
        let expr = match self.advance() {
            Tok::Int(n) => Expr::Int(n),
            Tok::Str(s) => Expr::Str(s),
            Tok::Name(name) => Expr::Name(name),
            Tok::Keyword("True") => Expr::Bool(true),
            Tok::Keyword("False") => Expr::Bool(false),
            Tok::Keyword("None") => Expr::None,
            Tok::Op("[") => Expr::List(self.arguments("]")?),
            Tok::Op("(") => {
                if self.eat_op(")") {
                    return Ok(Expr::Tuple(Vec::new()));
                }
                let first = self.expression()?;
                if self.eat_op(")") {
                    return Ok(first);
                }
                let mut items = vec![first];
                while self.eat_op(",") {
                    if self.is_op(")") {
                        break;
                    }
                    items.push(self.expression()?);
                }
                self.expect_op(")")?;
                Expr::Tuple(items)
            }
            _ => {
                self.pos -= 1;
                return Err(self.unexpected());
            }
        };
        Ok(expr)
    }
}

fn to_target(expr: &Expr) -> Option<Target> {
    match expr {
        Expr::Name(name) => Some(Target::Name(name.clone())),
        Expr::Index(object, index) => match object.as_ref() {
            Expr::Name(name) => Some(Target::Index(name.clone(), (**index).clone())),
            _ => None,
        },
        Expr::Tuple(items) => items
            .iter()
            .map(|item| match item {
                Expr::Name(name) => Some(name.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(Target::Unpack),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_small_program() {
        let program = parse(
            "def double(x):\n    return x * 2\n\nfor i in range(3):\n    if i > 0 and not i == 2:\n        print(double(i))\n    else:\n        pass\n",
        )
        .unwrap();
        assert_eq!(program.len(), 2);
        assert!(
            matches!(&program[0].kind, StmtKind::Def(name, params, body) if name == "double" && params == &["x"] && body.len() == 1)
        );
        assert!(
            matches!(&program[1].kind, StmtKind::For(Target::Name(i), _, body) if i == "i" && body.len() == 1)
        );
        assert_eq!(program[1].line, 4);
    }

    #[test]
    fn precedence_and_chained_comparisons() {
        let program = parse("x = 1 + 2 * 3 ** 2\ny = 0 < x < 10\n").unwrap();
        let StmtKind::Assign(_, Expr::Binary(BinOp::Add, _, right)) = &program[0].kind else {
            panic!("{:?}", program[0]);
        };
        assert!(matches!(right.as_ref(), Expr::Binary(BinOp::Mul, _, _)));
        assert!(matches!(
            &program[1].kind,
            StmtKind::Assign(_, Expr::And(_, _))
        ));
    }

    #[test]
    fn unpacking_indexing_and_methods() {
        let program =
            parse("x, y = position()\nl[0] = 5\nl.append(x)\nfor (a, b) in tours():\n    pass\n")
                .unwrap();
        assert!(
            matches!(&program[0].kind, StmtKind::Assign(Target::Unpack(names), _) if names.len() == 2)
        );
        assert!(
            matches!(&program[1].kind, StmtKind::Assign(Target::Index(name, _), _) if name == "l")
        );
        assert!(
            matches!(&program[2].kind, StmtKind::Expr(Expr::Method(o, m, _)) if o == "l" && m == "append")
        );
        assert!(matches!(
            &program[3].kind,
            StmtKind::For(Target::Unpack(_), _, _)
        ));
    }

    #[test]
    fn pedagogic_errors() {
        assert_eq!(
            parse("x = 7 / 2").unwrap_err().kind,
            ErrorKind::FloatDivision
        );
        assert_eq!(
            parse("if x:\nprint(1)").unwrap_err().kind,
            ErrorKind::ExpectedIndent
        );
        assert_eq!(
            parse("  x = 1").unwrap_err().kind,
            ErrorKind::UnexpectedIndent
        );
        assert_eq!(
            parse("if x\n    y = 1").unwrap_err().kind,
            ErrorKind::Expected(":")
        );
        assert_eq!(parse("3 = x").unwrap_err().kind, ErrorKind::InvalidTarget);
        assert!(matches!(
            parse("x = (1 + ").unwrap_err().kind,
            ErrorKind::UnexpectedToken(_)
        ));
    }
}
