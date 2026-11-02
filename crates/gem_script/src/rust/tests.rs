use super::*;
use crate::{ErrorKind, Host, HostReply, Step, Vm};

fn api() -> RustApi {
    use HostType::*;
    let pos = || Tuple(vec![Int, Int]);
    RustApi {
        functions: vec![
            RustFunction {
                names: vec!["dire_interne"],
                host: 0,
                params: vec![Str],
                ret: Unit,
            },
            RustFunction {
                names: vec!["attendre", "wait"],
                host: 1,
                params: vec![Int],
                ret: Unit,
            },
            RustFunction {
                names: vec!["position"],
                host: 2,
                params: vec![],
                ret: pos(),
            },
            RustFunction {
                names: vec!["prendre", "take"],
                host: 3,
                params: vec![],
                ret: Option(Box::new(Named("Objet"))),
            },
            RustFunction {
                names: vec!["deposer", "drop"],
                host: 4,
                params: vec![Named("Objet")],
                ret: Result(Box::new(Unit), Box::new(Named("Objet"))),
            },
            RustFunction {
                names: vec!["tours"],
                host: 5,
                params: vec![],
                ret: Vec(Box::new(pos())),
            },
            RustFunction {
                names: vec!["avancer"],
                host: 6,
                params: vec![Named("Direction")],
                ret: Unit,
            },
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
        print: 0,
    }
}

#[derive(Default)]
struct TestHost {
    output: Vec<String>,
    carrying: bool,
}

impl Host for TestHost {
    fn call(&mut self, function: u32, args: Vec<Value>) -> Result<HostReply, ErrorKind> {
        let done = |v| Ok(HostReply::Done(v));
        match function {
            0 => {
                self.output.push(args[0].to_string());
                done(Value::None)
            }
            1 => Ok(HostReply::Suspend),
            2 => done(Value::Tuple(vec![Value::Int(3), Value::Int(4)])),
            3 => {
                self.carrying = true;
                done(Value::Tuple(vec![Value::Bool(true), Value::Int(2)]))
            }
            4 => {
                let ok = self.carrying;
                self.carrying = false;
                done(if ok {
                    Value::Tuple(vec![Value::Bool(true), Value::Tuple(vec![])])
                } else {
                    Value::Tuple(vec![Value::Bool(false), args[0].clone()])
                })
            }
            5 => done(Value::List(vec![
                Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
                Value::Tuple(vec![Value::Int(5), Value::Int(6)]),
            ])),
            6 => {
                self.output.push(format!("avance {}", args[0]));
                done(Value::None)
            }
            _ => unreachable!(),
        }
    }
}

fn run(source: &str) -> Result<Vec<String>, ScriptError> {
    let program = compile_rust(source, &api())?;
    let mut vm = Vm::new(&program);
    let mut host = TestHost::default();
    for _ in 0..100_000 {
        match vm.run(&program, &mut host, 100) {
            Step::Finished => return Ok(host.output),
            Step::Failed(error) => return Err(error),
            Step::Suspended => vm.resume(Value::None),
            Step::OutOfFuel => {}
        }
    }
    panic!("le programme ne termine pas");
}

fn output(source: &str) -> String {
    match run(source) {
        Ok(lines) => lines.join("\n"),
        Err(e) => panic!("{}\n{source}", e.message(crate::Lang::French)),
    }
}

/// Code de l'erreur rustc (E0382…) et ligne.
fn error(source: &str) -> (String, u32) {
    match run(source) {
        Ok(lines) => panic!("aucune erreur, sortie : {lines:?}\n{source}"),
        Err(e) => match &e.kind {
            ErrorKind::Rust { code, .. } => (code.to_string(), e.line),
            other => (format!("{other:?}"), e.line),
        },
    }
}

fn code(source: &str) -> String {
    error(source).0
}

#[test]
fn detects_rust() {
    assert!(looks_like_rust("// golem\nfn main() {\n}"));
    assert!(looks_like_rust("pub fn aide(x: i64) {}"));
    assert!(!looks_like_rust("def main():\n    pass"));
    assert!(!looks_like_rust("fn = 3\nprint(fn)"));
}

#[test]
fn basics() {
    let source = r#"
fn carre(x: i64) -> i64 {
    x * x
}

fn main() {
    let mut total = 0;
    for i in 0..5 {
        total += carre(i);
    }
    let moitie = -7 / 2;
    let reste = -7 % 3;
    println!("total = {}, {} {} {}", total, moitie, reste, 2_i64.pow(10));
    let nom = "golem";
    let texte = format!("{nom}-{}", 7);
    println!("{texte} {} {:?} {}", true, "x", texte.len());
}
"#;
    assert_eq!(
        output(source),
        "total = 30, -3 -1 1024\ngolem-7 true \"x\" 7"
    );
}

#[test]
fn control_flow_and_expressions() {
    let source = r#"
fn signe(n: i64) -> &'static str {
    if n < 0 { "négatif" } else if n == 0 { "zéro" } else { "positif" }
}

fn main() {
    let mut n = 10;
    let pas = loop {
        n -= 3;
        if n < 0 {
            break n;
        }
    };
    let mut k = 0;
    while k < 3 {
        k += 1;
        if k == 2 {
            continue;
        }
        println!("k = {k}");
    }
    for i in (1..=3).rev() {
        print!("{i} ");
    }
    println!("{} {} {}", signe(pas), signe(0), signe(4));
    let x = 5;
    let x = x * 2;
    {
        let x = "dedans";
        println!("{x}");
    }
    println!("{x}");
}
"#;
    assert_eq!(
        output(source),
        "k = 1\nk = 3\n3 \n2 \n1 \nnégatif zéro positif\ndedans\n10"
    );
}

#[test]
fn match_enums_and_options() {
    let source = r#"
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Recolte,
    Livraison(i64),
    Attente,
}

fn decrire(m: Mode) -> String {
    match m {
        Mode::Recolte => String::from("récolte"),
        Mode::Livraison(n) if n > 5 => format!("grosse livraison de {n}"),
        Mode::Livraison(n) => format!("livraison de {n}"),
        Mode::Attente => "attente".to_string(),
    }
}

fn premier_pair(v: &Vec<i64>) -> Option<i64> {
    for &x in v {
        if x % 2 == 0 {
            return Some(x);
        }
    }
    None
}

fn main() {
    let modes = vec![Mode::Recolte, Mode::Livraison(3), Mode::Livraison(9), Mode::Attente];
    for m in &modes {
        println!("{}", decrire(*m));
    }
    println!("{:?} {}", modes[1], modes[0] == Mode::Recolte);
    let v = vec![3, 5, 8, 10];
    match premier_pair(&v) {
        Some(x) => println!("pair : {x}"),
        None => println!("aucun"),
    }
    if let Some(x) = premier_pair(&vec![1, 3]) {
        println!("{x}");
    } else {
        println!("rien");
    }
    let note = match v.len() {
        0 => "vide",
        1..=3 => "petit",
        _ => "grand",
    };
    println!("{note} {:?} {:?}", premier_pair(&v), Some((1, 2)));
    let d = Direction::Est;
    avancer(d);
    println!("{:?}", d);
}
"#;
    assert_eq!(
        output(source),
        "récolte\nlivraison de 3\ngrosse livraison de 9\nattente\nLivraison(3) true\npair : 8\nrien\ngrand Some(8) Some((1, 2))\navance 1\nEst"
    );
}

#[test]
fn structs_and_methods() {
    let source = r#"
#[derive(Debug, Clone, PartialEq)]
struct Point {
    x: i64,
    y: i64,
}

impl Point {
    fn new(x: i64, y: i64) -> Self {
        Point { x, y }
    }

    fn distance(&self, autre: &Point) -> i64 {
        (self.x - autre.x).abs() + (self.y - autre.y).abs()
    }

    fn deplacer(&mut self, dx: i64) {
        self.x += dx;
    }
}

struct Equipe {
    nom: String,
    points: Vec<Point>,
}

fn ajouter(e: &mut Equipe, p: Point) {
    e.points.push(p);
}

fn main() {
    let mut a = Point::new(1, 2);
    let b = Point { x: 4, y: 6 };
    println!("{}", a.distance(&b));
    a.deplacer(10);
    println!("{:?}", a);
    let mut e = Equipe { nom: "rouge".to_string(), points: Vec::new() };
    ajouter(&mut e, a.clone());
    ajouter(&mut e, b);
    e.points[0].y = 99;
    println!("{} {} {:?}", e.nom, e.points.len(), e.points);
    let mut t = (1, 2);
    t.0 = 5;
    println!("{:?}", t);
}
"#;
    assert_eq!(
        output(source),
        "7\nPoint { x: 11, y: 2 }\nrouge 2 [Point { x: 11, y: 99 }, Point { x: 4, y: 6 }]\n(5, 2)"
    );
}

#[test]
fn vectors_and_iterators() {
    let source = r#"
fn doubler(v: &mut Vec<i64>) {
    for x in v.iter_mut() {
        *x *= 2;
    }
}

fn main() {
    let mut v = vec![5, 1, 4];
    v.push(2);
    doubler(&mut v);
    v.sort();
    println!("{:?} {} {:?}", v, v.iter().sum::<i64>(), v.iter().max());
    let dernier = v.pop();
    println!("{:?} {:?} {}", dernier, v, v.contains(&8));
    for (i, x) in v.iter().enumerate() {
        print!("{i}:{x}");
    }
    let mut vide: Vec<i64> = Vec::new();
    println!("{:?} {}", vide.pop(), vide.is_empty());
    let zeros = vec![0; 3];
    println!("{:?} {:?} {:?}", zeros, v.first(), v.get(10));
    let t = tours();
    for (x, y) in t {
        print!("{}", x + y);
    }
}
"#;
    assert_eq!(
        output(source),
        "[2, 4, 8, 10] 24 Some(10)\nSome(10) [2, 4, 8] true\n0:2\n1:4\n2:8\nNone true\n[0, 0, 0] Some(2) None\n3\n11"
    );
}

#[test]
fn ownership_game_items() {
    // L'objet pris est déplacé dans deposer() : le golem ne l'a plus.
    let source = r#"
fn main() {
    if let Some(objet) = prendre() {
        let g = objet.grade();
        match deposer(objet) {
            Ok(()) => println!("déposé, grade {g}"),
            Err(o) => println!("raté {}", o.grade()),
        }
    }
    let r = deposer(prendre().unwrap());
    println!("{}", r.is_ok());
}
"#;
    assert_eq!(output(source), "déposé, grade 2\ntrue");

    let moved = r#"
fn main() {
    let objet = prendre().unwrap();
    deposer(objet);
    println!("{}", objet.grade());
}
"#;
    assert_eq!(error(moved), ("E0382".into(), 5));
    assert_eq!(
        code("fn main() {\n    let o = prendre().unwrap();\n    let copie = o.clone();\n}"),
        "E0599"
    );
    let in_loop = r#"
fn main() {
    let o = prendre().unwrap();
    loop {
        deposer(o);
    }
}
"#;
    assert_eq!(code(in_loop), "E0382");
    // Déplacé une seule fois avant de sortir de la boucle : correct.
    let ok = r#"
fn main() {
    let o = prendre().unwrap();
    loop {
        if deposer(o).is_ok() {
            println!("ok");
        }
        break;
    }
}
"#;
    assert_eq!(output(ok), "ok");
}

#[test]
fn ownership_and_borrowing() {
    assert_eq!(
        code("fn main() {\n    let v = vec![1];\n    let w = v;\n    println!(\"{:?}\", v);\n}"),
        "E0382"
    );
    // Les entiers sont Copy.
    assert_eq!(
        output("fn main() {\n    let a = 1;\n    let b = a;\n    println!(\"{a} {b}\");\n}"),
        "1 1"
    );
    // Prêter avec & ne déplace pas.
    let lend = r#"
fn taille(v: &Vec<i64>) -> usize {
    v.len()
}

fn main() {
    let v = vec![1, 2, 3];
    let n = taille(&v);
    println!("{n} {:?}", v);
}
"#;
    assert_eq!(output(lend), "3 [1, 2, 3]");
    assert_eq!(
        code("fn f(v: &Vec<i64>) {}\nfn main() {\n    let v = vec![1];\n    f(v);\n}"),
        "E0308"
    );
    assert_eq!(
        code("fn main() {\n    let v = vec![1];\n    v.push(2);\n}"),
        "E0596"
    );
    assert_eq!(code("fn main() {\n    let x = 1;\n    x = 2;\n}"), "E0384");
    assert_eq!(
        code("fn f(v: &Vec<i64>) {\n    v.push(1);\n}\nfn main() {}"),
        "E0596"
    );
    assert_eq!(
        code(
            "fn f(a: &mut Vec<i64>, b: &Vec<i64>) {}\nfn main() {\n    let mut v = vec![1];\n    f(&mut v, &v);\n}"
        ),
        "E0502"
    );
    assert_eq!(
        code(
            "fn f(a: &mut Vec<i64>, b: &mut Vec<i64>) {}\nfn main() {\n    let mut v = vec![1];\n    f(&mut v, &mut v);\n}"
        ),
        "E0499"
    );
    assert_eq!(
        code("fn main() {\n    let v = vec![vec![1]];\n    let w = v[0];\n}"),
        "E0507"
    );
    assert_eq!(
        code(
            "fn main() {\n    let s = String::from(\"a\");\n    if true {\n        let t = s;\n    }\n    println!(\"{s}\");\n}"
        ),
        "E0382"
    );
    // Réaffecter après un déplacement rend la variable utilisable.
    let reinit = r#"
fn main() {
    let mut v = vec![1];
    let w = v;
    v = vec![2];
    println!("{:?} {:?}", v, w);
}
"#;
    assert_eq!(output(reinit), "[2] [1]");
    let uninit = "fn main() {\n    let x: i64;\n    if true {\n        x = 1;\n    }\n    println!(\"{x}\");\n}";
    assert_eq!(code(uninit), "E0381");
    let init = "fn main() {\n    let x;\n    if true {\n        x = 1;\n    } else {\n        x = 2;\n    }\n    println!(\"{x}\");\n}";
    assert_eq!(output(init), "1");
}

#[test]
fn type_errors() {
    assert_eq!(code("fn main() {\n    let x: i64 = \"a\";\n}"), "E0308");
    assert_eq!(code("fn f() -> i64 {\n    5;\n}\nfn main() {}"), "E0308");
    assert_eq!(
        code("fn main() {\n    let v = vec![1];\n    println!(\"{}\", v);\n}"),
        "E0277"
    );
    assert_eq!(
        code(
            "struct P { x: i64 }\nfn main() {\n    let p = P { x: 1 };\n    println!(\"{:?}\", p);\n}"
        ),
        "E0277"
    );
    assert_eq!(code("fn main() {\n    let x = nombre;\n}"), "E0425");
    assert_eq!(
        code("fn main() {\n    let v: Vec<i64> = Vec::new();\n    v.pousser(1);\n}"),
        "E0599"
    );
    assert_eq!(
        code(
            "enum E { A, B }\nfn main() {\n    let e = E::A;\n    match e {\n        E::A => {}\n    }\n}"
        ),
        "E0004"
    );
    assert_eq!(code("fn main() {\n    if 1 {\n    }\n}"), "E0308");
    assert_eq!(
        code("fn main() {\n    avancer(Direction::Haut);\n}"),
        "E0599"
    );
    assert_eq!(
        code("fn f(a: i64) {}\nfn main() {\n    f(1, 2);\n}"),
        "E0061"
    );
    assert_eq!(
        code("fn main() {\n    let o = prendre();\n    let x = o?;\n}"),
        "E0277"
    );
    assert_eq!(code("fn principal() {}"), "E0601");
    assert_eq!(code("let x = 1;"), "");
    assert_eq!(error("fn main() {\n    let x = 1\n    let y = 2;\n}").1, 2);
}

#[test]
fn runtime_errors() {
    assert_eq!(
        code(
            "fn main() {\n    let v: Vec<i64> = vec![];\n    let x: Option<i64> = None;\n    x.unwrap();\n}"
        ),
        "UnwrapNone"
    );
    assert_eq!(
        code("fn main() {\n    panic!(\"stop {}\", 3);\n}"),
        "Panic(\"stop 3\")"
    );
    assert_eq!(
        code("fn main() {\n    let v = vec![1];\n    println!(\"{}\", v[5]);\n}"),
        "IndexOutOfRange { index: 5, len: 1 }"
    );
}

#[test]
fn question_mark_and_results() {
    let source = r#"
fn moitie(n: i64) -> Result<i64, String> {
    if n % 2 == 0 { Ok(n / 2) } else { Err(format!("{n} est impair")) }
}

fn quart(n: i64) -> Result<i64, String> {
    let m = moitie(n)?;
    moitie(m)
}

fn main() {
    println!("{:?} {:?}", quart(12), quart(6));
    let position = position();
    let (x, y) = position;
    println!("{x} {y} {:?}", position);
}
"#;
    assert_eq!(output(source), "Ok(3) Err(\"3 est impair\")\n3 4 (3, 4)");
}

#[test]
fn infinite_loops_do_not_block() {
    let program = compile_rust("fn main() {\n    loop {\n    }\n}", &api()).unwrap();
    let mut vm = Vm::new(&program);
    assert_eq!(
        vm.run(&program, &mut TestHost::default(), 50),
        Step::OutOfFuel
    );
}

#[test]
fn inspector_shows_rust_values() {
    let program = compile_rust(
        "fn main() {\n    let ok = true;\n    let p = Some((1, 2));\n    let v = vec![\"a\"];\n    attendre(1);\n}",
        &api(),
    )
    .unwrap();
    let mut vm = Vm::new(&program);
    assert_eq!(
        vm.run(&program, &mut TestHost::default(), 100),
        Step::Suspended
    );
    let shown: Vec<String> = vm
        .inspect(&program)
        .into_iter()
        .map(|(n, v)| format!("{n} = {v}"))
        .collect();
    assert_eq!(shown, vec!["ok = true", "p = Some((1, 2))", "v = [\"a\"]"]);
}

#[test]
fn trickier_programs() {
    let source = r#"
const LIMITE: i64 = 3;

#[derive(Debug, Clone, Copy)]
struct Case {
    x: i64,
    y: i64,
}

impl Case {
    fn new(x: i64, y: i64) -> Case {
        Case { x, y }
    }
    fn somme(self) -> i64 {
        self.x + self.y
    }
}

fn fact(n: i64) -> i64 {
    if n <= 1 { 1 } else { n * fact(n - 1) }
}

fn vider(pile: &mut Vec<i64>) -> i64 {
    let mut total = 0;
    while let Some(x) = pile.pop() {
        total += x;
    }
    total
}

fn premier(v: &[i64]) -> i64 {
    let Some(x) = v.first() else {
        return -1;
    };
    *x
}

fn main() {
    let mut pile = vec![1, 2, 3];
    let t = vider(&mut pile);
    println!("{t} {:?} {}", pile, fact(5));
    println!("{} {}", Case::new(2, 3).somme(), premier(&vec![]));
    let mut s = String::new();
    for i in 0..LIMITE {
        s.push_str(&i.to_string());
        s += "-";
    }
    let s2 = s.clone() + "fin";
    println!("{s2} {}", s.len());
    let mut grille = vec![vec![0; 2]; 2];
    grille[1][0] = 7;
    println!("{:?}", grille);
    let c = Case { x: 1, y: 2 };
    let Case { x, y: autre } = c;
    let d = c;
    println!("{x} {autre} {:?} {:?}", c, d);
    let r: Result<(), String> = Ok(());
    match r {
        Ok(()) => println!("ok"),
        Err(e) => println!("{e}"),
    }
    let mut compte = 0;
    for _ in 0..3 {
        let v = vec![compte];
        compte += v.len() as i64;
    }
    assert_eq!(compte, 3);
    assert!(compte > 2, "compte = {}", compte);
    let n = match compte { x if x > 2 => x * 10, _ => 0 };
    println!("{n}");
}
"#;
    assert_eq!(
        output(source),
        "6 [] 120\n5 -1\n0-1-2-fin 6\n[[0, 0], [7, 0]]\n1 2 Case { x: 1, y: 2 } Case { x: 1, y: 2 }\nok\n30"
    );
}

#[test]
fn error_messages_are_helpful() {
    let error = run("fn main() {\n    let objet = prendre().unwrap();\n    deposer(objet);\n    deposer(objet);\n}").unwrap_err();
    assert_eq!(
        error.message(crate::Lang::French),
        "Ligne 4 : la valeur de « objet » a été déplacée ligne 3 : elle n'est plus utilisable ici. Un objet du jeu est unique : une fois donné, le golem ne le possède plus. [E0382]"
    );
    let error = run("fn main() {\n    let v = vec![1];\n    v.push(2);\n}").unwrap_err();
    assert_eq!(
        error.message(crate::Lang::English),
        "Line 3: cannot borrow 'v' as mutable (&mut): declare it with let mut v [E0596]"
    );
}
