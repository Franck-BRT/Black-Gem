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

#[test]
fn iterators_and_closures() {
    let source = r#"
fn main() {
    let v = vec![5, 1, 4, 2, 8];
    let doubles: Vec<i64> = v.iter().map(|x| x * 2).collect();
    let pairs = v.iter().filter(|&&x| x % 2 == 0).count();
    let total: i64 = v.iter().sum();
    println!("{:?} {} {}", doubles, pairs, total);
    let grand = v.iter().copied().filter(|x| *x > 3).max();
    let premier = v.iter().position(|&x| x == 4);
    let trouve = v.iter().find(|x| **x > 4);
    println!("{:?} {:?} {:?}", grand, premier, trouve);
    let somme = (1..=4).fold(0, |acc, x| acc + x * x);
    let tous = v.iter().all(|&x| x > 0);
    let un = v.iter().any(|&x| x > 7);
    println!("{somme} {tous} {un}");
    for (i, x) in v.iter().enumerate().skip(1).take(2) {
        print!("{i}={x}");
    }
    let mut compteur = 0;
    v.iter().for_each(|x| compteur += x);
    println!("compteur {compteur}");
    let decalage = 10;
    let ajouter = |x: i64| x + decalage;
    println!("{} {}", ajouter(1), ajouter(5));
    let noms = vec!["rouge", "vert"];
    let couples: Vec<(i64, &str)> = (1..10).zip(noms.iter().copied()).collect();
    println!("{:?}", couples);
    let inverses: Vec<i64> = (0..5).rev().filter(|x| x % 2 == 0).collect();
    println!("{:?} {:?}", inverses, v.iter().min_by_key(|x| (**x - 4).abs()));
    let mut w = vec![3, 1, 2, 7, 6];
    w.retain(|&x| x != 7);
    w.sort_by_key(|x| -x);
    println!("{:?}", w);
    let texte = "golem";
    let n = texte.chars().filter(|c| *c == "o").count();
    println!("{n} {:?}", texte.chars().rev().collect::<Vec<&str>>());
}
"#;
    assert_eq!(
        output(source),
        "[10, 2, 8, 4, 16] 3 20\nSome(8) Some(2) Some(5)\n30 true true\n1=1\n2=4\ncompteur 20\n11 15\n[(1, \"rouge\"), (2, \"vert\")]\n[4, 2, 0] Some(4)\n[6, 3, 2, 1]\n1 [\"m\", \"e\", \"l\", \"o\", \"g\"]"
    );
}

#[test]
fn option_closures_and_lazy_iterators() {
    let source = r#"
fn main() {
    let o: Option<i64> = Some(3);
    let rien: Option<i64> = None;
    println!("{:?} {:?}", o.map(|x| x * 10), rien.map(|x| x * 10));
    println!("{} {}", rien.unwrap_or_else(|| 7), o.map_or(0, |x| x + 1));
    println!("{:?} {}", o.filter(|x| *x > 5), o.is_some_and(|x| x == 3));
    println!("{:?}", rien.ok_or("vide"));
    // find s'arrête au premier trouvé : la fermeture ne voit pas tous les éléments.
    let mut vus = 0;
    let premier = (1..100).inspect(|_| vus += 1).find(|x| x % 7 == 0);
    println!("{:?} {vus}", premier);
}
"#;
    assert_eq!(
        output(source),
        "Some(30) None\n7 4\nNone true\nErr(\"vide\")\nSome(7) 7"
    );
}

#[test]
fn traits_generics_and_display() {
    let source = r#"
use std::fmt;

trait Forme {
    fn aire(&self) -> i64;
    fn nom(&self) -> String {
        String::from("forme")
    }
    fn decrire(&self) -> String {
        format!("{} d'aire {}", self.nom(), self.aire())
    }
}

struct Carre {
    cote: i64,
}

struct Rect {
    l: i64,
    h: i64,
}

impl Forme for Carre {
    fn aire(&self) -> i64 {
        self.cote * self.cote
    }
    fn nom(&self) -> String {
        "carré".to_string()
    }
}

impl Forme for Rect {
    fn aire(&self) -> i64 {
        self.l * self.h
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}×{}", self.l, self.h)
    }
}

fn plus_grande<T: Forme>(a: &T, b: &T) -> i64 {
    a.aire().max(b.aire())
}

fn resume(x: &impl Forme) -> String {
    x.decrire()
}

fn appliquer<F: Fn(i64) -> i64>(f: F, x: i64) -> i64 {
    f(f(x))
}

fn premier<T: Copy>(v: &Vec<T>) -> T {
    v[0]
}

fn main() {
    let c = Carre { cote: 3 };
    let r = Rect { l: 2, h: 5 };
    println!("{} / {}", resume(&c), resume(&r));
    println!("{} {}", plus_grande(&c, &Carre { cote: 4 }), plus_grande(&r, &r));
    println!("{r} {}", r.to_string().len());
    let triple = |x| x * 3;
    println!("{} {}", appliquer(triple, 2), appliquer(|x| x + 1, 0));
    println!("{} {}", premier(&vec![7, 8]), premier(&vec!["a"]));
}
"#;
    assert_eq!(
        output(source),
        "carré d'aire 9 / forme d'aire 10\n16 10\n2×5 3\n18 2\n7 a"
    );
}

#[test]
fn m8_errors() {
    // Une fermeture rangée dans une variable ne peut pas modifier ce qu'elle capture.
    assert_eq!(
        code("fn main() {\n    let mut n = 0;\n    let f = |x: i64| n += x;\n    f(1);\n}"),
        ""
    );
    // move déplace la valeur dans la fermeture.
    assert_eq!(
        code(
            "fn main() {\n    let v = vec![1];\n    let f = move || v.len();\n    println!(\"{:?}\", v);\n}"
        ),
        "E0382"
    );
    assert_eq!(
        code("trait T { fn a(&self) -> i64; }\nstruct S;\nimpl T for S {}\nfn main() {}"),
        "E0046"
    );
    assert_eq!(
        code(
            "trait T { fn a(&self) -> i64; }\nstruct S;\nfn f<X: T>(x: &X) -> i64 { x.a() }\nfn main() {\n    f(&S);\n}"
        ),
        "E0277"
    );
    assert_eq!(
        code("fn main() {\n    let v = vec![1];\n    let s: i64 = v.map(|x| x).sum();\n}"),
        "E0599"
    );
    assert_eq!(
        code(
            "fn main() {\n    let v = vec![1];\n    let w: Vec<i64> = v.iter().map(|x| { return x; }).collect();\n}"
        ),
        ""
    );
    assert_eq!(
        code("struct P;\nfn main() {\n    println!(\"{}\", P);\n}"),
        "E0277"
    );
}

#[test]
fn nested_iterators_and_generic_chains() {
    let source = r#"
trait Neuf {
    fn neuf() -> Self;
    fn valeur(&self) -> i64;
}

#[derive(Debug, Clone, Copy)]
struct Jeton {
    n: i64,
}

impl Neuf for Jeton {
    fn neuf() -> Self {
        Jeton { n: 42 }
    }
    fn valeur(&self) -> i64 {
        self.n
    }
}

fn fabriquer<T: Neuf>() -> T {
    T::neuf()
}

fn doubler<T: Neuf>(x: &T) -> i64 {
    somme(&vec![x.valeur(), x.valeur()])
}

fn somme(v: &Vec<i64>) -> i64 {
    v.iter().sum()
}

fn compter_au_dessus(v: &Vec<i64>, seuil: i64) -> usize {
    v.iter().filter(|&&x| x > seuil).count()
}

fn main() {
    let j: Jeton = fabriquer();
    println!("{:?} {}", j, doubler(&j));
    let a = vec![1, 2, 3];
    let b = vec![2, 3, 4];
    let communs: Vec<i64> = a.iter().copied().filter(|x| b.iter().any(|y| y == x)).collect();
    println!("{:?} {}", communs, compter_au_dessus(&b, 2));
    let grille: Vec<Vec<i64>> = (0..3).map(|i| (0..3).map(|k| i * 3 + k).collect()).collect();
    let diag: i64 = grille.iter().enumerate().map(|(i, ligne)| ligne[i]).sum();
    println!("{:?} {diag}", grille[2]);
    let mut total = 0;
    for x in a.iter().map(|x| x * 100).take(2) {
        if x > 150 {
            break;
        }
        total += x;
    }
    println!("{total}");
}
"#;
    assert_eq!(
        output(source),
        "Jeton { n: 42 } 84\n[2, 3] 2\n[6, 7, 8] 12\n100"
    );
}
