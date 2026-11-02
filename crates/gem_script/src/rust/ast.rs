//! Arbre syntaxique du sous-ensemble Rust (avant vérification et traduction).

#[derive(Clone, Debug, PartialEq)]
pub enum TypeExpr {
    /// `i64`, `Vec<T>`, `Option<T>`, `Point`…
    Path(String, Vec<TypeExpr>),
    Ref(bool, Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    /// `[T; N]` ou `[T]` : traités comme des `Vec<T>`.
    Array(Box<TypeExpr>),
    /// `_` : à déduire.
    Infer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfKind {
    Value,
    Ref,
    RefMut,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub ty: TypeExpr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub line: u32,
    pub self_param: Option<(SelfKind, bool)>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub line: u32,
    pub fields: Vec<(String, TypeExpr)>,
    pub derives: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub line: u32,
    pub variants: Vec<(String, Vec<TypeExpr>)>,
    pub derives: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Impl {
        type_name: String,
        line: u32,
        methods: Vec<FnDecl>,
    },
    Const {
        name: String,
        line: u32,
        ty: TypeExpr,
        value: Expr,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Dernière expression sans point-virgule : la valeur du bloc.
    pub tail: Option<Box<Expr>>,
    pub line: u32,
    /// Ligne de l'accolade fermante.
    pub end_line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Let {
        pat: Pat,
        ty: Option<TypeExpr>,
        init: Option<Expr>,
        /// `let … else { … }`
        otherwise: Option<Block>,
        line: u32,
    },
    Expr(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Deref,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FmtMacro {
    Println,
    Print,
    Format,
    Panic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Str(String),
    Bool(bool),
    /// `a`, `Direction::Nord`, `Vec::new`…
    Path(Vec<String>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    ArrayRepeat(Box<Expr>, Box<Expr>),
    StructLit(Vec<String>, Vec<(String, Expr)>),
    Unary(UnOp, Box<Expr>),
    Ref(bool, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    CompoundAssign(BinOp, Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, TypeExpr),
    Call(Box<Expr>, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `p.x` ou `t.0`.
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Range(Option<Box<Expr>>, Option<Box<Expr>>, bool),
    Try(Box<Expr>),
    If(Box<Expr>, Block, Option<Box<Expr>>),
    IfLet(Pat, Box<Expr>, Block, Option<Box<Expr>>),
    While(Box<Expr>, Block),
    WhileLet(Pat, Box<Expr>, Block),
    Loop(Block),
    For(Pat, Box<Expr>, Block),
    Match(Box<Expr>, Vec<Arm>),
    Block(Block),
    Break(Option<Box<Expr>>),
    Continue,
    Return(Option<Box<Expr>>),
    Format(FmtMacro, String, Vec<Expr>),
    Assert(Box<Expr>, Option<Box<Expr>>),
    AssertEq(Box<Expr>, Box<Expr>, bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Wild,
    Bind {
        name: String,
        mutable: bool,
        by_ref: bool,
    },
    Int(i64),
    Str(String),
    Bool(bool),
    /// `a..=b` (ou `a..b`, borne exclue).
    Range(i64, i64, bool),
    Tuple(Vec<Pat>),
    /// `None`, `Direction::Nord`, `Mode::Attente`…
    Path(Vec<String>),
    /// `Some(p)`, `Ok(p)`, `Mode::Aller(p)`…
    TupleStruct(Vec<String>, Vec<Pat>),
    Struct(Vec<String>, Vec<(String, Pat)>),
    Or(Vec<Pat>),
    Ref(Box<Pat>),
}
