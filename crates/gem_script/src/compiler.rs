//! Compile l'arbre syntaxique en bytecode pour la machine virtuelle.

use std::collections::{BTreeSet, HashMap};

use crate::{
    ast::{BinOp, Expr, Stmt, StmtKind, Target},
    error::{ErrorKind, ScriptError, edit_distance},
    value::Value,
};

/// Fonctions intégrées au langage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Len,
    Range,
    Abs,
    Min,
    Max,
    Str,
}

impl Builtin {
    const ALL: [(&'static str, Builtin); 6] = [
        ("len", Builtin::Len),
        ("range", Builtin::Range),
        ("abs", Builtin::Abs),
        ("min", Builtin::Min),
        ("max", Builtin::Max),
        ("str", Builtin::Str),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, b)| *b == self).unwrap().0
    }

    fn lookup(name: &str) -> Option<Builtin> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, b)| *b)
    }
}

/// Une fonction fournie par le jeu, avec tous ses noms (français, anglais…).
#[derive(Clone, Debug)]
pub struct HostFunction {
    pub names: Vec<&'static str>,
    pub min_args: usize,
    pub max_args: usize,
}

/// Ce que le jeu met à disposition des programmes.
#[derive(Clone, Debug, Default)]
pub struct Api {
    pub functions: Vec<HostFunction>,
    pub constants: Vec<(Vec<&'static str>, Value)>,
}

impl Api {
    fn function(&self, name: &str) -> Option<u32> {
        self.functions
            .iter()
            .position(|f| f.names.contains(&name))
            .map(|i| i as u32)
    }

    fn constant(&self, name: &str) -> Option<&Value> {
        self.constants
            .iter()
            .find(|(names, _)| names.contains(&name))
            .map(|(_, v)| v)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Const(u32),
    LoadLocal(u32),
    StoreLocal(u32),
    LoadGlobal(u32),
    StoreGlobal(u32),
    Binary(BinOp),
    Neg,
    Not,
    Jump(u32),
    /// Saute si faux (et retire la valeur).
    JumpIfFalse(u32),
    /// Pour `and` / `or` : saute en gardant la valeur, sinon la retire.
    JumpIfFalseKeep(u32),
    JumpIfTrueKeep(u32),
    Pop,
    MakeList(u32),
    MakeTuple(u32),
    Index,
    StoreIndexLocal(u32),
    StoreIndexGlobal(u32),
    AppendLocal(u32),
    AppendGlobal(u32),
    Unpack(u32),
    GetIter,
    ForIter(u32),
    PopIter,
    Call(u32),
    CallBuiltin(Builtin, u32),
    CallHost(u32, u32),
    Return,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: u32,
    pub locals: Vec<String>,
    pub code: Vec<Op>,
    /// Ligne source de chaque instruction.
    pub lines: Vec<u32>,
}

/// Programme compilé, prêt à être exécuté.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// La fonction 0 est le programme principal.
    pub functions: Vec<Function>,
    pub constants: Vec<Value>,
    pub globals: Vec<String>,
}

pub fn compile(statements: &[Stmt], api: &Api) -> Result<Program, ScriptError> {
    let mut globals = BTreeSet::new();
    collect_assigned(statements, &mut globals);
    collect_global_declarations(statements, &mut globals);
    let globals: Vec<String> = globals.into_iter().collect();

    let mut compiler = Compiler {
        api,
        program: Program {
            functions: Vec::new(),
            constants: Vec::new(),
            globals: globals.clone(),
        },
        global_index: globals
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u32))
            .collect(),
    };
    compiler.function("<main>", &[], statements, None)?;
    Ok(compiler.program)
}

/// Noms qui reçoivent une valeur dans un bloc (sans descendre dans les `def`).
fn collect_assigned(statements: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::Assign(target, _) | StmtKind::For(target, _, _) => match target {
                Target::Name(n) => {
                    names.insert(n.clone());
                }
                Target::Unpack(ns) => names.extend(ns.iter().cloned()),
                Target::Index(..) => {}
            },
            StmtKind::AugAssign(n, _, _) | StmtKind::Def(n, _, _) => {
                names.insert(n.clone());
            }
            _ => {}
        }
        match &stmt.kind {
            StmtKind::If(branches, otherwise) => {
                for (_, body) in branches {
                    collect_assigned(body, names);
                }
                collect_assigned(otherwise, names);
            }
            StmtKind::While(_, body) | StmtKind::For(_, _, body) => collect_assigned(body, names),
            _ => {}
        }
    }
}

/// Noms déclarés `global` n'importe où (ils deviennent des variables globales).
fn collect_global_declarations(statements: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::Global(ns) => names.extend(ns.iter().cloned()),
            StmtKind::If(branches, otherwise) => {
                for (_, body) in branches {
                    collect_global_declarations(body, names);
                }
                collect_global_declarations(otherwise, names);
            }
            StmtKind::While(_, body) | StmtKind::For(_, _, body) | StmtKind::Def(_, _, body) => {
                collect_global_declarations(body, names)
            }
            _ => {}
        }
    }
}

fn declared_global(statements: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::Global(ns) => names.extend(ns.iter().cloned()),
            StmtKind::If(branches, otherwise) => {
                for (_, body) in branches {
                    declared_global(body, names);
                }
                declared_global(otherwise, names);
            }
            StmtKind::While(_, body) | StmtKind::For(_, _, body) => declared_global(body, names),
            _ => {}
        }
    }
}

struct Compiler<'a> {
    api: &'a Api,
    program: Program,
    global_index: HashMap<String, u32>,
}

enum Slot {
    Local(u32),
    Global(u32),
}

struct LoopContext {
    start: u32,
    is_for: bool,
    breaks: Vec<usize>,
}

struct FunctionBuilder {
    locals: Vec<String>,
    code: Vec<Op>,
    lines: Vec<u32>,
    line: u32,
    loops: Vec<LoopContext>,
    in_function: bool,
}

impl FunctionBuilder {
    fn emit(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.lines.push(self.line);
        self.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn patch(&mut self, at: usize, target: u32) {
        self.code[at] = match self.code[at] {
            Op::Jump(_) => Op::Jump(target),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::JumpIfFalseKeep(_) => Op::JumpIfFalseKeep(target),
            Op::JumpIfTrueKeep(_) => Op::JumpIfTrueKeep(target),
            Op::ForIter(_) => Op::ForIter(target),
            ref other => other.clone(),
        };
    }
}

impl Compiler<'_> {
    /// Compile une fonction (ou le programme principal) ; renvoie son indice.
    fn function(
        &mut self,
        name: &str,
        params: &[String],
        body: &[Stmt],
        line: Option<u32>,
    ) -> Result<u32, ScriptError> {
        let index = self.program.functions.len() as u32;
        // Réserve la place : les fonctions imbriquées seront ajoutées après.
        self.program.functions.push(Function {
            name: name.to_string(),
            params: params.len() as u32,
            locals: Vec::new(),
            code: Vec::new(),
            lines: Vec::new(),
        });

        let in_function = line.is_some();
        let mut locals: Vec<String> = params.to_vec();
        if in_function {
            let mut assigned = BTreeSet::new();
            collect_assigned(body, &mut assigned);
            let mut globals = BTreeSet::new();
            declared_global(body, &mut globals);
            for name in assigned {
                if !globals.contains(&name) && !locals.contains(&name) {
                    locals.push(name);
                }
            }
        }
        let mut builder = FunctionBuilder {
            locals,
            code: Vec::new(),
            lines: Vec::new(),
            line: line.unwrap_or(1),
            loops: Vec::new(),
            in_function,
        };
        self.block(&mut builder, body)?;
        let none = self.constant(Value::None);
        builder.emit(Op::Const(none));
        builder.emit(Op::Return);

        let function = &mut self.program.functions[index as usize];
        function.locals = builder.locals;
        function.code = builder.code;
        function.lines = builder.lines;
        Ok(index)
    }

    fn constant(&mut self, value: Value) -> u32 {
        if let Some(i) = self.program.constants.iter().position(|c| *c == value) {
            return i as u32;
        }
        self.program.constants.push(value);
        (self.program.constants.len() - 1) as u32
    }

    fn slot(&self, b: &FunctionBuilder, name: &str) -> Option<Slot> {
        if let Some(i) = b.locals.iter().position(|l| l == name) {
            return Some(Slot::Local(i as u32));
        }
        self.global_index.get(name).map(|&i| Slot::Global(i))
    }

    fn unknown(&self, b: &FunctionBuilder, name: &str) -> ScriptError {
        let mut candidates: Vec<&str> = b.locals.iter().map(String::as_str).collect();
        candidates.extend(self.program.globals.iter().map(String::as_str));
        candidates.extend(Builtin::ALL.iter().map(|(n, _)| *n));
        candidates.extend(["True", "False", "None"]);
        for f in &self.api.functions {
            candidates.extend(f.names.iter().copied());
        }
        for (names, _) in &self.api.constants {
            candidates.extend(names.iter().copied());
        }
        let suggestion = candidates
            .into_iter()
            .map(|c| (edit_distance(name, c), c))
            .filter(|(d, c)| *d <= 2.max(name.len() / 4) && *d < c.len())
            .min()
            .map(|(_, c)| c.to_string());
        ScriptError::new(
            b.line,
            ErrorKind::UnknownName {
                name: name.to_string(),
                suggestion,
            },
        )
    }

    fn store(&mut self, b: &mut FunctionBuilder, name: &str) -> Result<(), ScriptError> {
        match self.slot(b, name) {
            Some(Slot::Local(i)) => b.emit(Op::StoreLocal(i)),
            Some(Slot::Global(i)) => b.emit(Op::StoreGlobal(i)),
            None => return Err(self.unknown(b, name)),
        };
        Ok(())
    }

    fn block(&mut self, b: &mut FunctionBuilder, statements: &[Stmt]) -> Result<(), ScriptError> {
        for stmt in statements {
            b.line = stmt.line;
            self.statement(b, stmt)?;
        }
        Ok(())
    }

    fn statement(&mut self, b: &mut FunctionBuilder, stmt: &Stmt) -> Result<(), ScriptError> {
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                self.expr(b, expr)?;
                b.emit(Op::Pop);
            }
            StmtKind::Assign(target, value) => {
                if let Target::Index(name, index) = target {
                    self.expr(b, index)?;
                    self.expr(b, value)?;
                    match self.slot(b, name) {
                        Some(Slot::Local(i)) => b.emit(Op::StoreIndexLocal(i)),
                        Some(Slot::Global(i)) => b.emit(Op::StoreIndexGlobal(i)),
                        None => return Err(self.unknown(b, name)),
                    };
                } else {
                    self.expr(b, value)?;
                    self.assign(b, target)?;
                }
            }
            StmtKind::AugAssign(name, op, value) => {
                self.load_name(b, name)?;
                self.expr(b, value)?;
                b.emit(Op::Binary(*op));
                self.store(b, name)?;
            }
            StmtKind::If(branches, otherwise) => {
                let mut ends = Vec::new();
                for (condition, body) in branches {
                    self.expr(b, condition)?;
                    let skip = b.emit(Op::JumpIfFalse(0));
                    self.block(b, body)?;
                    ends.push(b.emit(Op::Jump(0)));
                    let here = b.here();
                    b.patch(skip, here);
                }
                self.block(b, otherwise)?;
                let end = b.here();
                for at in ends {
                    b.patch(at, end);
                }
            }
            StmtKind::While(condition, body) => {
                let start = b.here();
                self.expr(b, condition)?;
                let exit = b.emit(Op::JumpIfFalse(0));
                b.loops.push(LoopContext {
                    start,
                    is_for: false,
                    breaks: Vec::new(),
                });
                self.block(b, body)?;
                b.line = stmt.line;
                b.emit(Op::Jump(start));
                let end = b.here();
                b.patch(exit, end);
                for at in b.loops.pop().unwrap().breaks {
                    b.patch(at, end);
                }
            }
            StmtKind::For(target, iterable, body) => {
                self.expr(b, iterable)?;
                b.emit(Op::GetIter);
                let start = b.here();
                let exit = b.emit(Op::ForIter(0));
                self.assign(b, target)?;
                b.loops.push(LoopContext {
                    start,
                    is_for: true,
                    breaks: Vec::new(),
                });
                self.block(b, body)?;
                b.line = stmt.line;
                b.emit(Op::Jump(start));
                let end = b.here();
                b.patch(exit, end);
                for at in b.loops.pop().unwrap().breaks {
                    b.patch(at, end);
                }
            }
            StmtKind::Def(name, params, body) => {
                let index = self.function(name, params, body, Some(stmt.line))?;
                let constant = self.constant(Value::Function(index));
                b.emit(Op::Const(constant));
                self.store(b, name)?;
            }
            StmtKind::Return(value) => {
                if !b.in_function {
                    return Err(ScriptError::new(
                        stmt.line,
                        ErrorKind::ReturnOutsideFunction,
                    ));
                }
                match value {
                    Some(value) => self.expr(b, value)?,
                    None => {
                        let none = self.constant(Value::None);
                        b.emit(Op::Const(none));
                    }
                };
                b.emit(Op::Return);
            }
            StmtKind::Break => {
                let Some(context) = b.loops.last() else {
                    return Err(ScriptError::new(
                        stmt.line,
                        ErrorKind::LoopControlOutsideLoop("break"),
                    ));
                };
                if context.is_for {
                    b.emit(Op::PopIter);
                }
                let at = b.emit(Op::Jump(0));
                b.loops.last_mut().unwrap().breaks.push(at);
            }
            StmtKind::Continue => {
                let Some(context) = b.loops.last() else {
                    return Err(ScriptError::new(
                        stmt.line,
                        ErrorKind::LoopControlOutsideLoop("continue"),
                    ));
                };
                let start = context.start;
                b.emit(Op::Jump(start));
            }
            StmtKind::Global(_) | StmtKind::Pass => {}
        }
        Ok(())
    }

    fn assign(&mut self, b: &mut FunctionBuilder, target: &Target) -> Result<(), ScriptError> {
        match target {
            Target::Name(name) => self.store(b, name),
            Target::Unpack(names) => {
                b.emit(Op::Unpack(names.len() as u32));
                for name in names {
                    self.store(b, name)?;
                }
                Ok(())
            }
            Target::Index(..) => Err(ScriptError::new(b.line, ErrorKind::InvalidTarget)),
        }
    }

    fn load_name(&mut self, b: &mut FunctionBuilder, name: &str) -> Result<(), ScriptError> {
        match self.slot(b, name) {
            Some(Slot::Local(i)) => {
                b.emit(Op::LoadLocal(i));
            }
            Some(Slot::Global(i)) => {
                b.emit(Op::LoadGlobal(i));
            }
            None => {
                if let Some(value) = self.api.constant(name).cloned() {
                    let c = self.constant(value);
                    b.emit(Op::Const(c));
                } else if Builtin::lookup(name).is_some() || self.api.function(name).is_some() {
                    return Err(ScriptError::new(
                        b.line,
                        ErrorKind::NotAValue(name.to_string()),
                    ));
                } else {
                    return Err(self.unknown(b, name));
                }
            }
        }
        Ok(())
    }

    fn expr(&mut self, b: &mut FunctionBuilder, expr: &Expr) -> Result<(), ScriptError> {
        match expr {
            Expr::Int(n) => {
                let c = self.constant(Value::Int(*n));
                b.emit(Op::Const(c));
            }
            Expr::Str(s) => {
                let c = self.constant(Value::Str(s.clone()));
                b.emit(Op::Const(c));
            }
            Expr::Bool(v) => {
                let c = self.constant(Value::Bool(*v));
                b.emit(Op::Const(c));
            }
            Expr::None => {
                let c = self.constant(Value::None);
                b.emit(Op::Const(c));
            }
            Expr::Name(name) => self.load_name(b, name)?,
            Expr::List(items) | Expr::Tuple(items) => {
                for item in items {
                    self.expr(b, item)?;
                }
                let n = items.len() as u32;
                b.emit(if matches!(expr, Expr::List(_)) {
                    Op::MakeList(n)
                } else {
                    Op::MakeTuple(n)
                });
            }
            Expr::Binary(op, left, right) => {
                self.expr(b, left)?;
                self.expr(b, right)?;
                b.emit(Op::Binary(*op));
            }
            Expr::Neg(inner) => {
                self.expr(b, inner)?;
                b.emit(Op::Neg);
            }
            Expr::Not(inner) => {
                self.expr(b, inner)?;
                b.emit(Op::Not);
            }
            Expr::And(left, right) | Expr::Or(left, right) => {
                self.expr(b, left)?;
                let jump = b.emit(if matches!(expr, Expr::And(..)) {
                    Op::JumpIfFalseKeep(0)
                } else {
                    Op::JumpIfTrueKeep(0)
                });
                self.expr(b, right)?;
                let end = b.here();
                b.patch(jump, end);
            }
            Expr::Index(object, index) => {
                self.expr(b, object)?;
                self.expr(b, index)?;
                b.emit(Op::Index);
            }
            Expr::Method(object, method, args) => {
                if method != "append" {
                    return Err(ScriptError::new(
                        b.line,
                        ErrorKind::UnsupportedMethod(method.clone()),
                    ));
                }
                if args.len() != 1 {
                    return Err(ScriptError::new(
                        b.line,
                        ErrorKind::WrongArgCount {
                            name: "append".into(),
                            expected: "1".into(),
                            got: args.len(),
                        },
                    ));
                }
                self.expr(b, &args[0])?;
                match self.slot(b, object) {
                    Some(Slot::Local(i)) => b.emit(Op::AppendLocal(i)),
                    Some(Slot::Global(i)) => b.emit(Op::AppendGlobal(i)),
                    None => return Err(self.unknown(b, object)),
                };
            }
            Expr::Call(callee, args) => self.call(b, callee, args)?,
        }
        Ok(())
    }

    fn call(
        &mut self,
        b: &mut FunctionBuilder,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<(), ScriptError> {
        let argc = args.len() as u32;
        if let Expr::Name(name) = callee
            && self.slot(b, name).is_none()
        {
            if let Some(id) = self.api.function(name) {
                let f = &self.api.functions[id as usize];
                if args.len() < f.min_args || args.len() > f.max_args {
                    let expected = if f.min_args == f.max_args {
                        f.min_args.to_string()
                    } else if f.max_args == usize::MAX {
                        format!("{}+", f.min_args)
                    } else {
                        format!("{}-{}", f.min_args, f.max_args)
                    };
                    return Err(ScriptError::new(
                        b.line,
                        ErrorKind::WrongArgCount {
                            name: name.clone(),
                            expected,
                            got: args.len(),
                        },
                    ));
                }
                for arg in args {
                    self.expr(b, arg)?;
                }
                b.emit(Op::CallHost(id, argc));
                return Ok(());
            }
            if let Some(builtin) = Builtin::lookup(name) {
                for arg in args {
                    self.expr(b, arg)?;
                }
                b.emit(Op::CallBuiltin(builtin, argc));
                return Ok(());
            }
            return Err(self.unknown(b, name));
        }
        self.expr(b, callee)?;
        for arg in args {
            self.expr(b, arg)?;
        }
        b.emit(Op::Call(argc));
        Ok(())
    }
}
