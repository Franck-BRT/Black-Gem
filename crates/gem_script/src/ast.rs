//! Arbre syntaxique, commun aux langages du jeu (Python aujourd'hui, Rust ensuite).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    FloorDiv,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Int(i64),
    Str(String),
    Bool(bool),
    None,
    Name(String),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    /// `variable.methode(arguments)` (seule `append` existe pour l'instant).
    Method(String, String, Vec<Expr>),
}

/// Ce qui peut recevoir une valeur.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Name(String),
    Index(String, Expr),
    Unpack(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub line: u32,
    pub kind: StmtKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Expr(Expr),
    Assign(Target, Expr),
    AugAssign(String, BinOp, Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    For(Target, Expr, Vec<Stmt>),
    Def(String, Vec<String>, Vec<Stmt>),
    Return(Option<Expr>),
    Global(Vec<String>),
    Break,
    Continue,
    Pass,
}
