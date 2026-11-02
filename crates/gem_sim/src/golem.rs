//! Golems-drones : de petites unités volantes pilotées par un programme du joueur.
//!
//! Chaque golem exécute son programme dans la machine virtuelle de `gem_script`, avec un
//! budget d'instructions par tick. Les actions qui prennent du temps (voler, attendre,
//! prendre, déposer) suspendent le programme jusqu'à ce qu'elles soient terminées.

use gem_script::{
    Api, ErrorKind, Host, HostFunction, HostReply, HostType, Program, RustApi, RustEnum,
    RustFunction, RustOpaque, ScriptError, Step, Value, Vm, compile_python, compile_rust,
    looks_like_rust,
};

use crate::{
    TICKS_PER_SECOND,
    building::{BuildingKind, BuildingState, Buildings, SHOTS_PER_CRYSTAL, Storage},
    enemy::Enemy,
    gem::Item,
    geom::{Dir, Pos, Tile},
};

/// Coût d'un golem, en cristaux.
pub const GOLEM_COST: u32 = 15;
pub const MAX_GOLEMS: usize = 8;
/// Vitesse de vol, en unités fines par tick (3 cases/s).
pub const GOLEM_SPEED: i32 = 100;
/// Instructions exécutées par tick en mode normal.
const FUEL_PER_TICK: u32 = 40;
/// Un programme Rust est « compilé » : il exécute deux fois plus d'instructions par tick.
const RUST_FUEL_PER_TICK: u32 = 80;
/// En mode lent, une instruction tous les `SLOW_PERIOD` ticks : on voit le programme avancer.
const SLOW_PERIOD: u64 = 8;
/// Lignes gardées dans le journal d'un golem.
pub const LOG_LINES: usize = 12;

/// Valeurs renvoyées par `porte()` / `carrying()`.
pub const CARRY_CRYSTAL: i64 = 1;
pub const CARRY_GEM: i64 = 2;

/// Fonctions du jeu accessibles aux programmes (l'ordre donne leur identifiant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameFn {
    MoveTo,
    Step,
    Position,
    Hq,
    Towers,
    Drills,
    Ammo,
    HasGem,
    Enemies,
    NearestEnemy,
    Stock,
    Gems,
    Carrying,
    Take,
    TakeGem,
    Drop,
    Wait,
    Say,
    Core,
    Drilling,
    // Versions Rust : les objets portés sont des valeurs (ownership).
    TakeItem,
    TakeGemItem,
    DropItem,
}

/// Nom français, nom anglais, nombre d'arguments (min, max). Les fonctions réservées à Rust
/// ont des noms qu'un programme Python ne peut pas écrire.
const GAME_FUNCTIONS: [(GameFn, &[&str], usize, usize); 23] = [
    (GameFn::MoveTo, &["aller_vers", "move_to"], 1, 2),
    (GameFn::Step, &["avancer", "step"], 1, 1),
    (GameFn::Position, &["position"], 0, 0),
    (GameFn::Hq, &["qg", "hq"], 0, 0),
    (GameFn::Towers, &["tours", "towers"], 0, 0),
    (GameFn::Drills, &["foreuses", "drills"], 0, 0),
    (GameFn::Ammo, &["munitions", "ammo"], 1, 2),
    (GameFn::HasGem, &["a_gemme", "has_gem"], 1, 2),
    (GameFn::Enemies, &["ennemis", "enemies"], 0, 0),
    (
        GameFn::NearestEnemy,
        &["ennemi_proche", "nearest_enemy"],
        0,
        0,
    ),
    (GameFn::Stock, &["stock"], 0, 0),
    (GameFn::Gems, &["gemmes", "gems"], 0, 0),
    (GameFn::Carrying, &["porte", "carrying"], 0, 0),
    (GameFn::Take, &["prendre", "take"], 0, 0),
    (GameFn::TakeGem, &["prendre_gemme", "take_gem"], 0, 0),
    (GameFn::Drop, &["deposer", "déposer", "drop"], 0, 0),
    (GameFn::Wait, &["attendre", "wait"], 0, 1),
    (GameFn::Say, &["dire", "say", "print"], 0, usize::MAX),
    (GameFn::Core, &["coeur", "cœur", "core"], 0, 0),
    (GameFn::Drilling, &["forage", "drilling"], 0, 0),
    (GameFn::TakeItem, &["rust:prendre"], 0, 0),
    (GameFn::TakeGemItem, &["rust:prendre_gemme"], 0, 0),
    (GameFn::DropItem, &["rust:deposer"], 1, 1),
];

/// L'API des golems, pour le compilateur (et pour l'aide de l'éditeur).
pub fn golem_api() -> Api {
    Api {
        functions: GAME_FUNCTIONS
            .iter()
            .map(|(_, names, min, max)| HostFunction {
                names: names.to_vec(),
                min_args: *min,
                max_args: *max,
            })
            .collect(),
        constants: vec![
            (vec!["NORD", "NORTH"], Value::Int(0)),
            (vec!["EST", "EAST"], Value::Int(1)),
            (vec!["SUD", "SOUTH"], Value::Int(2)),
            (vec!["OUEST", "WEST"], Value::Int(3)),
            (vec!["CRISTAL", "CRYSTAL"], Value::Int(CARRY_CRYSTAL)),
            (vec!["GEMME", "GEM"], Value::Int(CARRY_GEM)),
        ],
    }
}

/// L'API des golems pour les programmes Rust : mêmes fonctions, avec des types. Les objets
/// portés sont des valeurs de type `Objet` : `prendre()` en donne un, `deposer(objet)` le
/// consomme (il est déplacé), et le compilateur interdit de s'en servir ensuite.
pub fn golem_rust_api() -> RustApi {
    use HostType::*;
    let pos = || Tuple(vec![Int, Int]);
    let f = |names: &[&'static str],
             game_fn: GameFn,
             params: std::vec::Vec<HostType>,
             ret: HostType| {
        RustFunction {
            names: names.to_vec(),
            host: game_fn as u32,
            params,
            ret,
        }
    };
    let item = || Named("Objet");
    RustApi {
        functions: vec![
            f(
                &["aller_vers", "move_to"],
                GameFn::MoveTo,
                vec![pos()],
                Unit,
            ),
            f(
                &["avancer", "step"],
                GameFn::Step,
                vec![Named("Direction")],
                Unit,
            ),
            f(&["position"], GameFn::Position, vec![], pos()),
            f(&["qg", "hq"], GameFn::Hq, vec![], pos()),
            f(
                &["tours", "towers"],
                GameFn::Towers,
                vec![],
                Vec(Box::new(pos())),
            ),
            f(
                &["foreuses", "drills"],
                GameFn::Drills,
                vec![],
                Vec(Box::new(pos())),
            ),
            f(&["munitions", "ammo"], GameFn::Ammo, vec![pos()], Int),
            f(&["a_gemme", "has_gem"], GameFn::HasGem, vec![pos()], Bool),
            f(&["ennemis", "enemies"], GameFn::Enemies, vec![], Int),
            f(
                &["ennemi_proche", "nearest_enemy"],
                GameFn::NearestEnemy,
                vec![],
                Option(Box::new(pos())),
            ),
            f(&["stock"], GameFn::Stock, vec![], Int),
            f(&["gemmes", "gems"], GameFn::Gems, vec![], Int),
            f(
                &["prendre", "take"],
                GameFn::TakeItem,
                vec![],
                Option(Box::new(item())),
            ),
            f(
                &["prendre_gemme", "take_gem"],
                GameFn::TakeGemItem,
                vec![],
                Option(Box::new(item())),
            ),
            f(
                &["deposer", "déposer", "sertir", "drop", "set_gem"],
                GameFn::DropItem,
                vec![item()],
                Result(Box::new(Unit), Box::new(item())),
            ),
            f(&["attendre", "wait"], GameFn::Wait, vec![Int], Unit),
            f(
                &["coeur", "cœur", "core"],
                GameFn::Core,
                vec![],
                Option(Box::new(pos())),
            ),
            f(&["forage", "drilling"], GameFn::Drilling, vec![], Int),
        ],
        enums: vec![RustEnum {
            names: vec!["Direction"],
            variants: vec![
                (vec!["Nord", "North"], Value::Int(0)),
                (vec!["Est", "East"], Value::Int(1)),
                (vec!["Sud", "South"], Value::Int(2)),
                (vec!["Ouest", "West"], Value::Int(3)),
            ],
        }],
        opaques: vec![RustOpaque {
            names: vec!["Objet", "Item"],
            fields: vec![(vec!["est_gemme", "is_gem"], Bool), (vec!["grade"], Int)],
        }],
        aliases: vec![(vec!["Position"], pos())],
        print: GameFn::Say as u32,
    }
}

/// Un objet porté, vu par un programme Rust : `(est_gemme, grade)`.
fn item_value(item: Item) -> Value {
    match item {
        Item::Gem(gem) => Value::Tuple(vec![Value::Bool(true), Value::Int(gem.grade as i64)]),
        _ => Value::Tuple(vec![Value::Bool(false), Value::Int(0)]),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GolemStatus {
    /// Pas de programme lancé.
    Idle,
    Running,
    /// Le programme est arrivé à la fin.
    Finished,
    Error(ScriptError),
}

/// Action en cours, qui suspend le programme.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    None,
    Move(Pos),
    Wait(u64),
    /// Action d'un tick : son résultat est rendu au programme au tick suivant.
    Done(Value),
}

#[derive(Clone, Debug)]
pub struct Golem {
    pub id: u32,
    pub pos: Pos,
    pub prev_pos: Pos,
    pub carrying: Option<Item>,
    /// Texte du programme (tel qu'écrit par le joueur).
    pub source: String,
    pub status: GolemStatus,
    /// Mode lent : une instruction à la fois, pour suivre le programme des yeux.
    pub slow: bool,
    /// Ligne en cours d'exécution.
    pub line: Option<u32>,
    /// Derniers messages affichés par `dire()` / `print()`.
    pub log: Vec<String>,
    /// Le programme est écrit en Rust.
    pub rust: bool,
    program: Option<Program>,
    vm: Option<Vm>,
    action: Action,
}

impl Golem {
    pub fn new(id: u32, pos: Pos) -> Self {
        Self {
            id,
            pos,
            prev_pos: pos,
            carrying: None,
            source: String::new(),
            status: GolemStatus::Idle,
            slow: false,
            line: None,
            log: Vec::new(),
            rust: false,
            program: None,
            vm: None,
            action: Action::None,
        }
    }

    /// Compile et lance un programme (en cas d'erreur, le golem l'affiche et reste à l'arrêt).
    pub fn start(&mut self, source: String) {
        self.stop();
        self.log.clear();
        self.rust = looks_like_rust(&source);
        let compiled = if self.rust {
            compile_rust(&source, &golem_rust_api())
        } else {
            compile_python(&source, &golem_api())
        };
        match compiled {
            Ok(program) => {
                self.vm = Some(Vm::new(&program));
                self.program = Some(program);
                self.status = GolemStatus::Running;
            }
            Err(error) => {
                self.line = Some(error.line);
                self.status = GolemStatus::Error(error);
            }
        }
        self.source = source;
    }

    pub fn stop(&mut self) {
        self.vm = None;
        self.program = None;
        self.action = Action::None;
        self.line = None;
        self.status = GolemStatus::Idle;
    }

    /// Variables globales du programme en cours (pour l'inspecteur de l'éditeur).
    pub fn variables(&self) -> Vec<(String, String)> {
        match (&self.vm, &self.program) {
            (Some(vm), Some(program)) => vm.inspect(program),
            _ => Vec::new(),
        }
    }

    fn say(&mut self, text: String) {
        let text: String = text.chars().take(80).collect();
        self.log.push(text);
        if self.log.len() > LOG_LINES {
            self.log.remove(0);
        }
    }
}

/// Ce que le golem voit et touche pendant son tick.
pub struct World<'a> {
    pub buildings: &'a mut Buildings,
    pub storage: Storage<'a>,
    pub enemies: &'a [Enemy],
    pub map_size: u32,
    pub tick: u64,
    /// Centre du Cœur (s'il y en a un).
    pub core: Option<Tile>,
    /// Avancement du forage, en pour cent.
    pub drilling_percent: u32,
    /// Vitesse de vol, en unités fines par tick.
    pub golem_speed: i32,
}

impl Golem {
    /// Un tick de vie du golem : poursuivre l'action en cours, puis faire tourner le programme.
    pub fn update(&mut self, world: &mut World) {
        self.prev_pos = self.pos;
        if self.status != GolemStatus::Running {
            return;
        }
        let finished_action = match &self.action {
            Action::None => None,
            Action::Move(target) => {
                let target = *target;
                self.pos
                    .move_towards(target, world.golem_speed)
                    .then_some(Value::None)
            }
            Action::Wait(until) => (world.tick >= *until).then_some(Value::None),
            Action::Done(value) => Some(value.clone()),
        };
        if let Some(value) = finished_action {
            self.action = Action::None;
            if let Some(vm) = &mut self.vm {
                vm.resume(value);
            }
        }
        if self.action != Action::None {
            return;
        }

        let fuel = if !self.slow {
            if self.rust {
                RUST_FUEL_PER_TICK
            } else {
                FUEL_PER_TICK
            }
        } else if world.tick.is_multiple_of(SLOW_PERIOD) {
            1
        } else {
            0
        };
        let (Some(mut vm), Some(program)) = (self.vm.take(), self.program.take()) else {
            return;
        };
        let step = vm.run(&program, &mut GolemHost { golem: self, world }, fuel);
        self.line = vm.current_line(&program);
        match step {
            Step::Failed(error) => {
                self.line = Some(error.line);
                self.status = GolemStatus::Error(error);
            }
            Step::Finished => self.status = GolemStatus::Finished,
            Step::OutOfFuel | Step::Suspended => {}
        }
        self.vm = Some(vm);
        self.program = Some(program);
    }
}

struct GolemHost<'g, 'w, 'a> {
    golem: &'g mut Golem,
    world: &'w mut World<'a>,
}

fn game_error(french: impl Into<String>, english: impl Into<String>) -> ErrorKind {
    ErrorKind::Game {
        french: french.into(),
        english: english.into(),
    }
}

fn position_value(tile: Tile) -> Value {
    Value::Tuple(vec![Value::Int(tile.x as i64), Value::Int(tile.y as i64)])
}

/// Une position : `(x, y)` ou deux nombres `x, y`.
fn position_arg(name: &str, args: &[Value]) -> Result<Tile, ErrorKind> {
    let pair = match args {
        [Value::Tuple(items) | Value::List(items)] => items.as_slice(),
        _ => args,
    };
    match pair {
        [x, y] => match (x.as_int(), y.as_int()) {
            (Some(x), Some(y)) => Ok(Tile::new(
                x.clamp(-1, i32::MAX as i64) as i32,
                y.clamp(-1, i32::MAX as i64) as i32,
            )),
            _ => Err(ErrorKind::WrongArgType {
                name: name.into(),
                expected: "position",
                got: if x.as_int().is_none() {
                    x.type_name()
                } else {
                    y.type_name()
                },
            }),
        },
        _ => Err(ErrorKind::WrongArgType {
            name: name.into(),
            expected: "position",
            got: args.first().map_or("None", Value::type_name),
        }),
    }
}

impl GolemHost<'_, '_, '_> {
    fn tile(&self) -> Tile {
        self.golem.pos.tile()
    }

    fn hq_tile(&self) -> Tile {
        let hq = self.world.buildings.hq().expect("le QG existe");
        Tile::new(hq.tile.x + hq.size() / 2, hq.tile.y + hq.size() / 2)
    }

    fn on_hq(&self) -> bool {
        self.world
            .buildings
            .at(self.tile())
            .is_some_and(|b| b.kind == BuildingKind::Hq)
    }

    fn tower_state(&self, name: &str, args: &[Value]) -> Result<(u32, u32, bool), ErrorKind> {
        let tile = position_arg(name, args)?;
        match self.world.buildings.at(tile).map(|b| &b.state) {
            Some(BuildingState::Tower {
                crystals,
                charge,
                gem,
                ..
            }) => Ok((*crystals, *charge, gem.is_some())),
            _ => Err(game_error(
                format!("il n'y a pas de tour en ({}, {})", tile.x, tile.y),
                format!("there is no tower at ({}, {})", tile.x, tile.y),
            )),
        }
    }

    fn positions_of(&self, kind: BuildingKind) -> Value {
        Value::List(
            self.world
                .buildings
                .iter()
                .filter(|b| b.kind == kind)
                .map(|b| position_value(b.tile))
                .collect(),
        )
    }

    /// Termine l'appel au tick suivant avec ce résultat (une action prend au moins un tick).
    fn after_a_tick(&mut self, value: Value) -> HostReply {
        self.golem.action = Action::Done(value);
        HostReply::Suspend
    }
}

impl Host for GolemHost<'_, '_, '_> {
    fn call(&mut self, function: u32, args: Vec<Value>) -> Result<HostReply, ErrorKind> {
        let (game_fn, names, _, _) = GAME_FUNCTIONS[function as usize];
        let name = names[0];
        let done = |v: Value| Ok(HostReply::Done(v));
        match game_fn {
            GameFn::MoveTo => {
                let tile = position_arg(name, &args)?;
                let max = self.world.map_size as i32 - 1;
                let tile = Tile::new(tile.x.clamp(0, max), tile.y.clamp(0, max));
                self.golem.action = Action::Move(tile.center());
                Ok(HostReply::Suspend)
            }
            GameFn::Step => {
                let dir = match args[0].as_int() {
                    Some(0) => Dir::North,
                    Some(1) => Dir::East,
                    Some(2) => Dir::South,
                    Some(3) => Dir::West,
                    _ => {
                        return Err(game_error(
                            "avancer() attend une direction : NORD, SUD, EST ou OUEST",
                            "step() expects a direction: NORTH, SOUTH, EAST or WEST",
                        ));
                    }
                };
                let target = self.tile().step(dir);
                let max = self.world.map_size as i32 - 1;
                let target = Tile::new(target.x.clamp(0, max), target.y.clamp(0, max));
                self.golem.action = Action::Move(target.center());
                Ok(HostReply::Suspend)
            }
            GameFn::Position => done(position_value(self.tile())),
            GameFn::Hq => done(position_value(self.hq_tile())),
            GameFn::Towers => done(self.positions_of(BuildingKind::Tower)),
            GameFn::Drills => done(self.positions_of(BuildingKind::Drill)),
            GameFn::Ammo => {
                let (crystals, charge, _) = self.tower_state(name, &args)?;
                done(Value::Int((crystals * SHOTS_PER_CRYSTAL + charge) as i64))
            }
            GameFn::HasGem => {
                let (_, _, has_gem) = self.tower_state(name, &args)?;
                done(Value::Bool(has_gem))
            }
            GameFn::Enemies => done(Value::Int(self.world.enemies.len() as i64)),
            GameFn::NearestEnemy => {
                let me = self.golem.pos;
                let nearest = self
                    .world
                    .enemies
                    .iter()
                    .min_by_key(|e| (e.pos.distance_sq(me), e.id))
                    .map(|e| position_value(e.pos.tile()));
                done(nearest.unwrap_or(Value::None))
            }
            GameFn::Stock => done(Value::Int(*self.world.storage.crystals as i64)),
            GameFn::Gems => done(Value::Int(self.world.storage.gems.len() as i64)),
            GameFn::Carrying => done(match self.golem.carrying {
                None => Value::None,
                Some(Item::Gem(_)) => Value::Int(CARRY_GEM),
                Some(_) => Value::Int(CARRY_CRYSTAL),
            }),
            GameFn::Take => {
                let taken = if self.golem.carrying.is_some() {
                    None
                } else if self.on_hq() {
                    // Au QG : un éclat de cristal, pris sur la réserve.
                    (*self.world.storage.crystals > 0).then(|| {
                        *self.world.storage.crystals -= 1;
                        Item::Shard
                    })
                } else {
                    let tile = self.tile();
                    self.world.buildings.take_at(tile)
                };
                self.golem.carrying = taken.or(self.golem.carrying);
                Ok(self.after_a_tick(Value::Bool(taken.is_some())))
            }
            GameFn::TakeGem => {
                let best = self
                    .world
                    .storage
                    .gems
                    .iter()
                    .enumerate()
                    .max_by_key(|(i, g)| (g.grade, std::cmp::Reverse(*i)))
                    .map(|(i, _)| i);
                let taken = match best {
                    Some(i) if self.golem.carrying.is_none() && self.on_hq() => {
                        Some(self.world.storage.gems.remove(i))
                    }
                    _ => None,
                };
                if let Some(gem) = taken {
                    self.golem.carrying = Some(Item::Gem(gem));
                }
                Ok(self.after_a_tick(Value::Bool(taken.is_some())))
            }
            GameFn::TakeItem | GameFn::TakeGemItem => {
                // Un golem qui porte déjà un objet le « reprend » : utile si son programme
                // a été relancé pendant qu'il portait quelque chose.
                if let Some(item) = self.golem.carrying {
                    return Ok(self.after_a_tick(item_value(item)));
                }
                let reply = if game_fn == GameFn::TakeItem {
                    self.call(GameFn::Take as u32, Vec::new())?
                } else {
                    self.call(GameFn::TakeGem as u32, Vec::new())?
                };
                debug_assert!(matches!(reply, HostReply::Suspend));
                let value = self.golem.carrying.map_or(Value::None, item_value);
                Ok(self.after_a_tick(value))
            }
            GameFn::DropItem => {
                let objet = args.into_iter().next().unwrap_or(Value::None);
                self.call(GameFn::Drop as u32, Vec::new())?;
                let dropped = self.golem.carrying.is_none();
                let value = if dropped {
                    Value::Tuple(vec![Value::Bool(true), Value::Tuple(Vec::new())])
                } else {
                    Value::Tuple(vec![Value::Bool(false), objet])
                };
                Ok(self.after_a_tick(value))
            }
            GameFn::Drop => {
                let dropped = match self.golem.carrying {
                    Some(item) => {
                        let tile = self.tile();
                        self.world
                            .buildings
                            .give_at(tile, item, &mut self.world.storage)
                    }
                    None => false,
                };
                if dropped {
                    self.golem.carrying = None;
                }
                Ok(self.after_a_tick(Value::Bool(dropped)))
            }
            GameFn::Wait => {
                let seconds = match args.first() {
                    None => 1,
                    Some(v) => v.as_int().ok_or(ErrorKind::WrongArgType {
                        name: name.into(),
                        expected: "int",
                        got: v.type_name(),
                    })?,
                };
                let ticks = (seconds.clamp(0, 600) as u64 * TICKS_PER_SECOND as u64).max(1);
                self.golem.action = Action::Wait(self.world.tick + ticks);
                Ok(HostReply::Suspend)
            }
            GameFn::Core => done(self.world.core.map_or(Value::None, position_value)),
            GameFn::Drilling => done(Value::Int(self.world.drilling_percent as i64)),
            GameFn::Say => {
                let text: Vec<String> = args.iter().map(Value::to_string).collect();
                self.golem.say(text.join(" "));
                done(Value::None)
            }
        }
    }
}

/// Programmes d'exemple proposés dans l'éditeur (français, anglais).
pub const TEMPLATES: [(&str, &str, &str); 4] = [
    (
        "Ravitailleur",
        "# Ravitailleur : apporte des munitions aux tours qui en manquent.\n\
         while True:\n    \
             for x, y in tours():\n        \
                 if munitions(x, y) < 12:\n            \
                     aller_vers(qg())\n            \
                     if prendre():\n                \
                         aller_vers(x, y)\n                \
                         deposer()\n                \
                         dire(\"Munitions livrées en\", x, y)\n    \
             attendre(1)\n",
        "# Supplier: brings ammo to towers that are running low.\n\
         while True:\n    \
             for x, y in towers():\n        \
                 if ammo(x, y) < 12:\n            \
                     move_to(hq())\n            \
                     if take():\n                \
                         move_to(x, y)\n                \
                         drop()\n                \
                         say(\"Ammo delivered at\", x, y)\n    \
             wait(1)\n",
    ),
    (
        "Sertisseur",
        "# Sertisseur : place les meilleures gemmes du QG dans les tours qui n'en ont pas.\n\
         while True:\n    \
             for x, y in tours():\n        \
                 if not a_gemme(x, y) and gemmes() > 0:\n            \
                     aller_vers(qg())\n            \
                     prendre_gemme()\n            \
                     aller_vers(x, y)\n            \
                     deposer()\n            \
                     dire(\"Gemme sertie en\", x, y)\n    \
             attendre(2)\n",
        "# Jeweller: puts the best gems from the HQ into towers that have none.\n\
         while True:\n    \
             for x, y in towers():\n        \
                 if not has_gem(x, y) and gems() > 0:\n            \
                     move_to(hq())\n            \
                     take_gem()\n            \
                     move_to(x, y)\n            \
                     drop()\n            \
                     say(\"Gem set at\", x, y)\n    \
             wait(2)\n",
    ),
    (
        "Mineur",
        "# Mineur : apporte les cristaux des foreuses au Grand foreur.\n\
         while True:\n    \
             for x, y in foreuses():\n        \
                 aller_vers(x, y)\n        \
                 if prendre():\n            \
                     aller_vers(coeur())\n            \
                     deposer()\n            \
                     dire(\"Forage :\", forage(), \"%\")\n    \
             attendre(1)\n",
        "# Miner: brings crystals from the drills to the Great drill.\n\
         while True:\n    \
             for x, y in drills():\n        \
                 move_to(x, y)\n        \
                 if take():\n            \
                     move_to(core())\n            \
                     drop()\n            \
                     say(\"Drilling:\", drilling(), \"%\")\n    \
             wait(1)\n",
    ),
    (
        "Patrouille",
        "# Patrouille : fait le tour du QG et signale les ennemis.\n\
         x, y = qg()\n\
         while True:\n    \
             for dx, dy in [(-6, -6), (6, -6), (6, 6), (-6, 6)]:\n        \
                 aller_vers(x + dx, y + dy)\n        \
                 if ennemis() > 0:\n            \
                     dire(\"Attention :\", ennemis(), \"ennemis, le plus proche en\", ennemi_proche())\n",
        "# Patrol: circles the HQ and reports enemies.\n\
         x, y = hq()\n\
         while True:\n    \
             for dx, dy in [(-6, -6), (6, -6), (6, 6), (-6, 6)]:\n        \
                 move_to(x + dx, y + dy)\n        \
                 if enemies() > 0:\n            \
                     say(\"Warning:\", enemies(), \"enemies, nearest at\", nearest_enemy())\n",
    ),
];

/// Les mêmes programmes d'exemple, en Rust (Acte II) : les objets portés sont des valeurs
/// qui se **déplacent**.
pub const RUST_TEMPLATES: [(&str, &str, &str); 4] = [
    (
        "Ravitailleur",
        r#"// Ravitailleur (Rust) : munitions pour les tours.
// prendre() donne un Objet : le golem le possède.
// deposer(cristal) le déplace dans la tour.
fn main() {
    loop {
        for tour in tours() {
            if munitions(tour) < 12 {
                aller_vers(qg());
                if let Some(cristal) = prendre() {
                    aller_vers(tour);
                    if deposer(cristal).is_ok() {
                        println!("Munitions livrées en {:?}", tour);
                    }
                }
            }
        }
        attendre(1);
    }
}
"#,
        r#"// Supplier (Rust): ammo for the towers.
// take() gives an Item: the golem owns it.
// drop(crystal) moves it into the tower.
fn main() {
    loop {
        for tower in towers() {
            if ammo(tower) < 12 {
                move_to(hq());
                if let Some(crystal) = take() {
                    move_to(tower);
                    if drop(crystal).is_ok() {
                        println!("Ammo delivered at {:?}", tower);
                    }
                }
            }
        }
        wait(1);
    }
}
"#,
    ),
    (
        "Sertisseur",
        r#"// Sertisseur (Rust) : une gemme est une valeur unique.
// Après sertir(gemme), le golem ne l'a plus : essaie
// gemme.grade() juste après, le compilateur refuse.
fn main() {
    loop {
        for tour in tours() {
            if !a_gemme(tour) && gemmes() > 0 {
                aller_vers(qg());
                if let Some(gemme) = prendre_gemme() {
                    // Lire n'est pas prendre :
                    let grade = gemme.grade();
                    aller_vers(tour);
                    match sertir(gemme) {
                        Ok(()) => println!("Gemme {grade} sertie"),
                        Err(g) => println!("Refusée : {}", g.grade()),
                    }
                }
            }
        }
        attendre(2);
    }
}
"#,
        r#"// Jeweller (Rust): a gem is a unique value.
// After set_gem(gem), the golem no longer has it: try
// gem.grade() right after, the compiler refuses.
fn main() {
    loop {
        for tower in towers() {
            if !has_gem(tower) && gems() > 0 {
                move_to(hq());
                if let Some(gem) = take_gem() {
                    // Reading is not taking:
                    let grade = gem.grade();
                    move_to(tower);
                    match set_gem(gem) {
                        Ok(()) => println!("Grade {grade} gem set"),
                        Err(g) => println!("Refused: {}", g.grade()),
                    }
                }
            }
        }
        wait(2);
    }
}
"#,
    ),
    (
        "Mineur",
        r#"// Mineur (Rust) : apporte les cristaux des foreuses au Grand foreur.
// coeur() renvoie une Option : None sans Cœur.
fn main() {
    let Some(coeur) = coeur() else {
        println!("Pas de Cœur sur cette planète");
        return;
    };
    loop {
        for foreuse in foreuses() {
            aller_vers(foreuse);
            if let Some(cristal) = prendre() {
                aller_vers(coeur);
                let _ = deposer(cristal);
                println!("Forage : {} %", forage());
            }
        }
        attendre(1);
    }
}
"#,
        r#"// Miner (Rust): brings crystals from the drills to the Great drill.
// core() returns an Option: None while there is no Core.
fn main() {
    let Some(core) = core() else {
        println!("No Core on this planet");
        return;
    };
    loop {
        for drill in drills() {
            move_to(drill);
            if let Some(crystal) = take() {
                move_to(core);
                let _ = drop(crystal);
                println!("Drilling: {} %", drilling());
            }
        }
        wait(1);
    }
}
"#,
    ),
    (
        "Patrouille",
        r#"// Patrouille (Rust) : fait le tour du QG et signale les ennemis.
// &parcours : la boucle l'emprunte sans le prendre.
const RAYON: i64 = 6;

fn coins(centre: Position) -> Vec<Position> {
    let (x, y) = centre;
    vec![
        (x - RAYON, y - RAYON),
        (x + RAYON, y - RAYON),
        (x + RAYON, y + RAYON),
        (x - RAYON, y + RAYON),
    ]
}

fn main() {
    let parcours = coins(qg());
    loop {
        for &coin in &parcours {
            aller_vers(coin);
            if let Some(ennemi) = ennemi_proche() {
                println!("{} ennemis, proche : {:?}", ennemis(), ennemi);
            }
        }
    }
}
"#,
        r#"// Patrol (Rust): circles the HQ and reports enemies.
// &route: the loop borrows it without taking it.
const RADIUS: i64 = 6;

fn corners(center: Position) -> Vec<Position> {
    let (x, y) = center;
    vec![
        (x - RADIUS, y - RADIUS),
        (x + RADIUS, y - RADIUS),
        (x + RADIUS, y + RADIUS),
        (x - RADIUS, y + RADIUS),
    ]
}

fn main() {
    let route = corners(hq());
    loop {
        for &corner in &route {
            move_to(corner);
            if let Some(enemy) = nearest_enemy() {
                println!("{} enemies, nearest: {:?}", enemies(), enemy);
            }
        }
    }
}
"#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_compile_in_both_languages() {
        let api = golem_api();
        for (name, french, english) in TEMPLATES {
            compile_python(french, &api).unwrap_or_else(|e| panic!("{name} (fr) : {e}"));
            compile_python(english, &api).unwrap_or_else(|e| panic!("{name} (en) : {e}"));
        }
        let rust = golem_rust_api();
        for (name, french, english) in RUST_TEMPLATES {
            assert!(looks_like_rust(french) && looks_like_rust(english));
            compile_rust(french, &rust).unwrap_or_else(|e| panic!("{name} (fr) : {e}"));
            compile_rust(english, &rust).unwrap_or_else(|e| panic!("{name} (en) : {e}"));
        }
    }
}
