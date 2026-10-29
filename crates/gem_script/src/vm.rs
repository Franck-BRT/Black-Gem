//! Machine virtuelle : exécute le bytecode instruction par instruction, avec un budget
//! (« carburant ») par appel, pour que le jeu garde la main et reste déterministe.

use crate::{
    ast::BinOp,
    compiler::{Builtin, Op, Program},
    error::{ErrorKind, ScriptError},
    value::{Value, range_len},
};

/// Profondeur maximale d'appels de fonctions.
const MAX_DEPTH: usize = 64;
/// Taille maximale d'une liste ou d'un texte.
const MAX_LEN: usize = 10_000;

/// Réponse du jeu à l'appel d'une de ses fonctions.
pub enum HostReply {
    /// Résultat immédiat.
    Done(Value),
    /// Action qui prend du temps (se déplacer, attendre…) : le programme est suspendu
    /// jusqu'à ce que le jeu appelle [`Vm::resume`].
    Suspend,
}

/// Le jeu, vu depuis le programme.
pub trait Host {
    fn call(&mut self, function: u32, args: Vec<Value>) -> Result<HostReply, ErrorKind>;
}

/// Résultat d'une période d'exécution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Budget épuisé : le programme continuera au prochain appel de [`Vm::run`].
    OutOfFuel,
    /// En attente de la fin d'une action du jeu.
    Suspended,
    Finished,
    Failed(ScriptError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Iter {
    Seq(Vec<Value>, usize),
    Range(i64, i64, i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    function: u32,
    pc: u32,
    locals: Vec<Option<Value>>,
    stack_base: usize,
    iter_base: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Running,
    Waiting,
    Finished,
    Failed,
}

/// État d'exécution d'un programme (le programme lui-même est passé à chaque appel).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vm {
    frames: Vec<Frame>,
    stack: Vec<Value>,
    iters: Vec<Iter>,
    globals: Vec<Option<Value>>,
    state: State,
}

type Exec<T> = Result<T, ErrorKind>;

impl Vm {
    pub fn new(program: &Program) -> Self {
        Self {
            frames: vec![Frame {
                function: 0,
                pc: 0,
                locals: vec![None; program.functions[0].locals.len()],
                stack_base: 0,
                iter_base: 0,
            }],
            stack: Vec::new(),
            iters: Vec::new(),
            globals: vec![None; program.globals.len()],
            state: State::Running,
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, State::Finished | State::Failed)
    }

    pub fn is_waiting(&self) -> bool {
        self.state == State::Waiting
    }

    /// Ligne en cours d'exécution (pour la surligner dans l'éditeur).
    pub fn current_line(&self, program: &Program) -> Option<u32> {
        let frame = self.frames.last()?;
        let lines = &program.functions[frame.function as usize].lines;
        // Pendant une attente, le pc est déjà sur l'instruction suivante.
        let pc = if self.state == State::Waiting {
            frame.pc.saturating_sub(1)
        } else {
            frame.pc
        };
        lines.get(pc as usize).or(lines.last()).copied()
    }

    /// Valeurs des variables globales (pour l'inspecteur de l'éditeur).
    pub fn globals<'a>(
        &'a self,
        program: &'a Program,
    ) -> impl Iterator<Item = (&'a str, &'a Value)> {
        program
            .globals
            .iter()
            .zip(&self.globals)
            .filter_map(|(name, value)| Some((name.as_str(), value.as_ref()?)))
            .filter(|(_, value)| !matches!(value, Value::Function(_)))
    }

    /// Reprend après une action du jeu, avec son résultat.
    pub fn resume(&mut self, value: Value) {
        if self.state == State::Waiting {
            self.stack.push(value);
            self.state = State::Running;
        }
    }

    /// Exécute au plus `fuel` instructions.
    pub fn run(&mut self, program: &Program, host: &mut impl Host, fuel: u32) -> Step {
        match self.state {
            State::Finished => return Step::Finished,
            State::Waiting => return Step::Suspended,
            State::Failed => return Step::Finished,
            State::Running => {}
        }
        for _ in 0..fuel {
            let line = self.current_line(program).unwrap_or(0);
            match self.step(program, host) {
                Ok(()) => {}
                Err(kind) => {
                    self.state = State::Failed;
                    return Step::Failed(ScriptError::new(line, kind));
                }
            }
            match self.state {
                State::Running => {}
                State::Waiting => return Step::Suspended,
                State::Finished => return Step::Finished,
                State::Failed => unreachable!(),
            }
        }
        Step::OutOfFuel
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("pile vide : bytecode invalide")
    }

    fn pop_args(&mut self, n: u32) -> Vec<Value> {
        let at = self.stack.len() - n as usize;
        self.stack.split_off(at)
    }

    fn step(&mut self, program: &Program, host: &mut impl Host) -> Exec<()> {
        let frame = self.frames.last_mut().unwrap();
        let function = &program.functions[frame.function as usize];
        let op = function.code[frame.pc as usize].clone();
        frame.pc += 1;

        match op {
            Op::Const(i) => self.stack.push(program.constants[i as usize].clone()),
            Op::LoadLocal(i) => {
                let frame = self.frames.last().unwrap();
                let value = frame.locals[i as usize].clone().ok_or_else(|| {
                    let name = &program.functions[frame.function as usize].locals[i as usize];
                    ErrorKind::UndefinedVariable(name.clone())
                })?;
                self.stack.push(value);
            }
            Op::StoreLocal(i) => {
                let value = self.pop();
                self.frames.last_mut().unwrap().locals[i as usize] = Some(value);
            }
            Op::LoadGlobal(i) => {
                let value = self.globals[i as usize].clone().ok_or_else(|| {
                    ErrorKind::UndefinedVariable(program.globals[i as usize].clone())
                })?;
                self.stack.push(value);
            }
            Op::StoreGlobal(i) => {
                let value = self.pop();
                self.globals[i as usize] = Some(value);
            }
            Op::Binary(op) => {
                let right = self.pop();
                let left = self.pop();
                self.stack.push(binary(op, left, right)?);
            }
            Op::Neg => {
                let value = self.pop();
                let n = value.as_int().ok_or(ErrorKind::TypeMismatch {
                    op: "-",
                    left: "",
                    right: value.type_name(),
                })?;
                self.stack
                    .push(Value::Int(n.checked_neg().ok_or(ErrorKind::Overflow)?));
            }
            Op::Not => {
                let value = self.pop();
                self.stack.push(Value::Bool(!value.truthy()));
            }
            Op::Jump(target) => self.frames.last_mut().unwrap().pc = target,
            Op::JumpIfFalse(target) => {
                if !self.pop().truthy() {
                    self.frames.last_mut().unwrap().pc = target;
                }
            }
            Op::JumpIfFalseKeep(target) | Op::JumpIfTrueKeep(target) => {
                let jump_when = matches!(op, Op::JumpIfTrueKeep(_));
                if self.stack.last().unwrap().truthy() == jump_when {
                    self.frames.last_mut().unwrap().pc = target;
                } else {
                    self.pop();
                }
            }
            Op::Pop => {
                self.pop();
            }
            Op::MakeList(n) => {
                let items = self.pop_args(n);
                self.stack.push(Value::List(items));
            }
            Op::MakeTuple(n) => {
                let items = self.pop_args(n);
                self.stack.push(Value::Tuple(items));
            }
            Op::Index => {
                let index = self.pop();
                let object = self.pop();
                self.stack.push(index_value(&object, &index)?);
            }
            Op::StoreIndexLocal(slot) | Op::StoreIndexGlobal(slot) => {
                let value = self.pop();
                let index = self.pop();
                let target = match op {
                    Op::StoreIndexLocal(_) => {
                        &mut self.frames.last_mut().unwrap().locals[slot as usize]
                    }
                    _ => &mut self.globals[slot as usize],
                };
                let Some(Value::List(items)) = target else {
                    let kind = target.as_ref().map_or("None", Value::type_name);
                    return Err(ErrorKind::NotIndexable(kind));
                };
                let i = list_index(&index, items.len())?;
                items[i] = value;
            }
            Op::AppendLocal(slot) | Op::AppendGlobal(slot) => {
                let value = self.pop();
                let target = match op {
                    Op::AppendLocal(_) => {
                        &mut self.frames.last_mut().unwrap().locals[slot as usize]
                    }
                    _ => &mut self.globals[slot as usize],
                };
                let Some(Value::List(items)) = target else {
                    let kind = target.as_ref().map_or("None", Value::type_name);
                    return Err(ErrorKind::WrongArgType {
                        name: "append".into(),
                        expected: "list",
                        got: kind,
                    });
                };
                if items.len() >= MAX_LEN {
                    return Err(ErrorKind::TooBig);
                }
                items.push(value);
                self.stack.push(Value::None);
            }
            Op::Unpack(n) => {
                let value = self.pop();
                let items = match value {
                    Value::List(items) | Value::Tuple(items) => items,
                    other => return Err(ErrorKind::NotIterable(other.type_name())),
                };
                if items.len() != n as usize {
                    return Err(ErrorKind::UnpackMismatch {
                        expected: n as usize,
                        got: items.len(),
                    });
                }
                self.stack.extend(items.into_iter().rev());
            }
            Op::GetIter => {
                let iter = match self.pop() {
                    Value::List(items) | Value::Tuple(items) => Iter::Seq(items, 0),
                    Value::Range(start, stop, step) => Iter::Range(start, stop, step),
                    Value::Str(s) => {
                        Iter::Seq(s.chars().map(|c| Value::Str(c.to_string())).collect(), 0)
                    }
                    other => return Err(ErrorKind::NotIterable(other.type_name())),
                };
                self.iters.push(iter);
            }
            Op::ForIter(exit) => {
                let next = match self.iters.last_mut().unwrap() {
                    Iter::Seq(items, i) => {
                        let item = items.get(*i).cloned();
                        *i += 1;
                        item
                    }
                    Iter::Range(current, stop, step) => {
                        if range_len(*current, *stop, *step) > 0 {
                            let value = *current;
                            *current += *step;
                            Some(Value::Int(value))
                        } else {
                            None
                        }
                    }
                };
                match next {
                    Some(value) => self.stack.push(value),
                    None => {
                        self.iters.pop();
                        self.frames.last_mut().unwrap().pc = exit;
                    }
                }
            }
            Op::PopIter => {
                self.iters.pop();
            }
            Op::Call(argc) => {
                let args = self.pop_args(argc);
                let callee = self.pop();
                let Value::Function(index) = callee else {
                    return Err(ErrorKind::NotCallable(callee.type_name()));
                };
                let function = &program.functions[index as usize];
                if args.len() != function.params as usize {
                    return Err(ErrorKind::WrongArgCount {
                        name: function.name.clone(),
                        expected: function.params.to_string(),
                        got: args.len(),
                    });
                }
                if self.frames.len() >= MAX_DEPTH {
                    return Err(ErrorKind::RecursionTooDeep);
                }
                let mut locals: Vec<Option<Value>> = args.into_iter().map(Some).collect();
                locals.resize(function.locals.len(), None);
                self.frames.push(Frame {
                    function: index,
                    pc: 0,
                    locals,
                    stack_base: self.stack.len(),
                    iter_base: self.iters.len(),
                });
            }
            Op::CallBuiltin(builtin, argc) => {
                let args = self.pop_args(argc);
                self.stack.push(call_builtin(builtin, args)?);
            }
            Op::CallHost(id, argc) => {
                let args = self.pop_args(argc);
                match host.call(id, args)? {
                    HostReply::Done(value) => self.stack.push(value),
                    HostReply::Suspend => self.state = State::Waiting,
                }
            }
            Op::Return => {
                let result = self.pop();
                let frame = self.frames.pop().unwrap();
                self.stack.truncate(frame.stack_base);
                self.iters.truncate(frame.iter_base);
                if self.frames.is_empty() {
                    self.state = State::Finished;
                } else {
                    self.stack.push(result);
                }
            }
        }
        Ok(())
    }
}

fn floor_div(a: i64, b: i64) -> Exec<i64> {
    if b == 0 {
        return Err(ErrorKind::DivisionByZero);
    }
    let q = a.checked_div(b).ok_or(ErrorKind::Overflow)?;
    Ok(if a % b != 0 && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    })
}

fn floor_mod(a: i64, b: i64) -> Exec<i64> {
    if b == 0 {
        return Err(ErrorKind::DivisionByZero);
    }
    let r = a.checked_rem(b).ok_or(ErrorKind::Overflow)?;
    Ok(if r != 0 && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    })
}

fn repeat<T: Clone>(items: &[T], times: i64) -> Exec<Vec<T>> {
    let times = times.max(0) as usize;
    if items.len().saturating_mul(times) > MAX_LEN {
        return Err(ErrorKind::TooBig);
    }
    Ok((0..times).flat_map(|_| items.iter().cloned()).collect())
}

fn binary(op: BinOp, left: Value, right: Value) -> Exec<Value> {
    use Value::*;
    let mismatch = || ErrorKind::TypeMismatch {
        op: op.symbol(),
        left: left.type_name(),
        right: right.type_name(),
    };
    if let (Some(a), Some(b)) = (left.as_int(), right.as_int()) {
        let overflow = ErrorKind::Overflow;
        return Ok(match op {
            BinOp::Add => Int(a.checked_add(b).ok_or(overflow)?),
            BinOp::Sub => Int(a.checked_sub(b).ok_or(overflow)?),
            BinOp::Mul => Int(a.checked_mul(b).ok_or(overflow)?),
            BinOp::FloorDiv => Int(floor_div(a, b)?),
            BinOp::Mod => Int(floor_mod(a, b)?),
            BinOp::Pow => {
                if b < 0 {
                    return Err(ErrorKind::NegativePower);
                }
                let exp = u32::try_from(b).map_err(|_| ErrorKind::Overflow)?;
                Int(a.checked_pow(exp).ok_or(overflow)?)
            }
            BinOp::Eq => Bool(a == b),
            BinOp::Ne => Bool(a != b),
            BinOp::Lt => Bool(a < b),
            BinOp::Le => Bool(a <= b),
            BinOp::Gt => Bool(a > b),
            BinOp::Ge => Bool(a >= b),
        });
    }
    match op {
        BinOp::Eq => return Ok(Bool(left == right)),
        BinOp::Ne => return Ok(Bool(left != right)),
        _ => {}
    }
    match (op, &left, &right) {
        (BinOp::Add, Str(a), Str(b)) => {
            if a.len() + b.len() > MAX_LEN {
                return Err(ErrorKind::TooBig);
            }
            Ok(Str(format!("{a}{b}")))
        }
        (BinOp::Add, List(a), List(b)) | (BinOp::Add, Tuple(a), Tuple(b)) => {
            if a.len() + b.len() > MAX_LEN {
                return Err(ErrorKind::TooBig);
            }
            let joined = [a.as_slice(), b.as_slice()].concat();
            Ok(if matches!(left, List(_)) {
                List(joined)
            } else {
                Tuple(joined)
            })
        }
        (BinOp::Mul, Str(s), n) | (BinOp::Mul, n, Str(s)) if n.as_int().is_some() => {
            let chars: Vec<char> = s.chars().collect();
            Ok(Str(repeat(&chars, n.as_int().unwrap())?
                .into_iter()
                .collect()))
        }
        (BinOp::Mul, List(items), n) | (BinOp::Mul, n, List(items)) if n.as_int().is_some() => {
            Ok(List(repeat(items, n.as_int().unwrap())?))
        }
        (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, Str(a), Str(b)) => Ok(Bool(match op {
            BinOp::Lt => a < b,
            BinOp::Le => a <= b,
            BinOp::Gt => a > b,
            _ => a >= b,
        })),
        _ => Err(mismatch()),
    }
}

fn list_index(index: &Value, len: usize) -> Exec<usize> {
    let i = index.as_int().ok_or(ErrorKind::WrongArgType {
        name: "[ ]".into(),
        expected: "int",
        got: index.type_name(),
    })?;
    let actual = if i < 0 { i + len as i64 } else { i };
    if actual < 0 || actual >= len as i64 {
        return Err(ErrorKind::IndexOutOfRange { index: i, len });
    }
    Ok(actual as usize)
}

fn index_value(object: &Value, index: &Value) -> Exec<Value> {
    match object {
        Value::List(items) | Value::Tuple(items) => {
            Ok(items[list_index(index, items.len())?].clone())
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            Ok(Value::Str(
                chars[list_index(index, chars.len())?].to_string(),
            ))
        }
        other => Err(ErrorKind::NotIndexable(other.type_name())),
    }
}

fn int_arg(name: Builtin, value: &Value) -> Exec<i64> {
    value.as_int().ok_or(ErrorKind::WrongArgType {
        name: name.name().into(),
        expected: "int",
        got: value.type_name(),
    })
}

fn call_builtin(builtin: Builtin, args: Vec<Value>) -> Exec<Value> {
    let wrong_count = |expected: &str| ErrorKind::WrongArgCount {
        name: builtin.name().into(),
        expected: expected.into(),
        got: args.len(),
    };
    match builtin {
        Builtin::Len => {
            let [value] = args.as_slice() else {
                return Err(wrong_count("1"));
            };
            let len = match value {
                Value::List(items) | Value::Tuple(items) => items.len() as i64,
                Value::Str(s) => s.chars().count() as i64,
                Value::Range(a, b, c) => range_len(*a, *b, *c),
                other => {
                    return Err(ErrorKind::WrongArgType {
                        name: "len".into(),
                        expected: "list",
                        got: other.type_name(),
                    });
                }
            };
            Ok(Value::Int(len))
        }
        Builtin::Range => {
            let ints: Vec<i64> = args
                .iter()
                .map(|a| int_arg(builtin, a))
                .collect::<Exec<_>>()?;
            let (start, stop, step) = match ints.as_slice() {
                [stop] => (0, *stop, 1),
                [start, stop] => (*start, *stop, 1),
                [start, stop, step] => (*start, *stop, *step),
                _ => return Err(wrong_count("1-3")),
            };
            if step == 0 {
                return Err(ErrorKind::Game {
                    french: "range() : le pas ne peut pas être 0".into(),
                    english: "range(): the step cannot be 0".into(),
                });
            }
            Ok(Value::Range(start, stop, step))
        }
        Builtin::Abs => {
            let [value] = args.as_slice() else {
                return Err(wrong_count("1"));
            };
            let n = int_arg(builtin, value)?;
            Ok(Value::Int(n.checked_abs().ok_or(ErrorKind::Overflow)?))
        }
        Builtin::Min | Builtin::Max => {
            let items: Vec<Value> = match args.as_slice() {
                [Value::List(items) | Value::Tuple(items)] => items.clone(),
                [] => return Err(wrong_count("1+")),
                _ => args.clone(),
            };
            let ints: Vec<i64> = items
                .iter()
                .map(|a| int_arg(builtin, a))
                .collect::<Exec<_>>()?;
            let result = if builtin == Builtin::Min {
                ints.iter().min()
            } else {
                ints.iter().max()
            };
            result
                .map(|n| Value::Int(*n))
                .ok_or_else(|| wrong_count("1+"))
        }
        Builtin::Str => {
            let [value] = args.as_slice() else {
                return Err(wrong_count("1"));
            };
            Ok(Value::Str(value.to_string()))
        }
    }
}
