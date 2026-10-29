//! Langage de programmation du jeu : un sous-ensemble de Python, compilé en bytecode et
//! exécuté par une machine virtuelle sûre et déterministe.
//!
//! - **sûr** : un programme ne peut que lire et agir sur le jeu via l'[`Api`] fournie ;
//!   il s'exécute par petites tranches (« carburant »), ne peut ni bloquer le jeu ni
//!   épuiser la mémoire ;
//! - **déterministe** : uniquement des entiers, aucune horloge ni hasard ;
//! - **pédagogique** : messages d'erreur en français et en anglais, avec suggestions.
//!
//! L'arbre syntaxique est indépendant de la syntaxe : un analyseur pour un sous-ensemble
//! de Rust pourra produire le même arbre et réutiliser compilateur et machine virtuelle.

pub mod ast;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod value;
pub mod vm;

pub use compiler::{Api, HostFunction, Program};
pub use error::{ErrorKind, Lang, ScriptError};
pub use value::Value;
pub use vm::{Host, HostReply, Step, Vm};

/// Compile un programme Python.
pub fn compile_python(source: &str, api: &Api) -> Result<Program, ScriptError> {
    let statements = parser::parse(source)?;
    compiler::compile(&statements, api)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hôte de test : `afficher(x)` / `print(x)` note la valeur, `attendre()` suspend.
    #[derive(Default)]
    struct TestHost {
        output: Vec<String>,
    }

    fn api() -> Api {
        Api {
            functions: vec![
                HostFunction {
                    names: vec!["afficher", "print"],
                    min_args: 0,
                    max_args: usize::MAX,
                },
                HostFunction {
                    names: vec!["attendre", "wait"],
                    min_args: 0,
                    max_args: 1,
                },
            ],
            constants: vec![(vec!["NORD", "NORTH"], Value::Int(0))],
        }
    }

    impl Host for TestHost {
        fn call(&mut self, function: u32, args: Vec<Value>) -> Result<HostReply, ErrorKind> {
            match function {
                0 => {
                    let text: Vec<String> = args.iter().map(Value::to_string).collect();
                    self.output.push(text.join(" "));
                    Ok(HostReply::Done(Value::None))
                }
                _ => Ok(HostReply::Suspend),
            }
        }
    }

    fn run(source: &str) -> Result<Vec<String>, ScriptError> {
        let program = compile_python(source, &api())?;
        let mut vm = Vm::new(&program);
        let mut host = TestHost::default();
        for _ in 0..10_000 {
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
        run(source).unwrap().join("\n")
    }

    fn error(source: &str) -> ErrorKind {
        run(source).unwrap_err().kind
    }

    #[test]
    fn arithmetic_like_python() {
        assert_eq!(
            output("print(1 + 2 * 3, 7 // 2, -7 // 2, -7 % 3, 2 ** 10)"),
            "7 3 -4 2 1024"
        );
        assert_eq!(
            output("print('ab' * 3, 'x' + 'y', [1] + [2], 3 > 2 > 1)"),
            "ababab xy [1, 2] True"
        );
    }

    #[test]
    fn control_flow() {
        let source = "
total = 0
for i in range(10):
    if i % 2 == 0:
        continue
    if i > 7:
        break
    total += i
n = 3
while n > 0:
    n -= 1
print(total, n)
";
        assert_eq!(output(source), "16 0");
    }

    #[test]
    fn functions_recursion_and_globals() {
        let source = "
compteur = 0
def fib(n):
    global compteur
    compteur += 1
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
print(fib(10), compteur)
";
        assert_eq!(output(source), "55 177");
    }

    #[test]
    fn lists_tuples_and_unpacking() {
        let source = "
points = []
for x, y in [(1, 2), (3, 4)]:
    points.append(x * y)
points[0] = 10
a, b = points
print(points, a + b, len(points), points[-1], max(points), min(3, 1, 2))
";
        assert_eq!(output(source), "[10, 12] 22 2 12 12 1");
    }

    #[test]
    fn host_constants_and_bilingual_names() {
        assert_eq!(
            output("afficher(NORD, NORTH)\nwait()\nprint('ok')"),
            "0 0\nok"
        );
    }

    #[test]
    fn helpful_compile_errors() {
        assert!(matches!(
            error("aficher(1)"),
            ErrorKind::UnknownName { suggestion: Some(s), .. } if s == "afficher"
        ));
        assert_eq!(
            error("x = afficher"),
            ErrorKind::NotAValue("afficher".into())
        );
        assert!(matches!(
            error("attendre(1, 2)"),
            ErrorKind::WrongArgCount { .. }
        ));
        assert_eq!(error("return 1"), ErrorKind::ReturnOutsideFunction);
        assert_eq!(error("break"), ErrorKind::LoopControlOutsideLoop("break"));
    }

    #[test]
    fn helpful_runtime_errors() {
        assert_eq!(error("print(1 // 0)"), ErrorKind::DivisionByZero);
        assert_eq!(
            error("l = [1]\nprint(l[3])"),
            ErrorKind::IndexOutOfRange { index: 3, len: 1 }
        );
        assert!(matches!(
            error("print(1 + 'a')"),
            ErrorKind::TypeMismatch { .. }
        ));
        assert_eq!(
            error("def f():\n    return f()\nf()"),
            ErrorKind::RecursionTooDeep
        );
        assert_eq!(
            error("print(x)\nx = 1"),
            ErrorKind::UndefinedVariable("x".into())
        );
        let failure = run("x = 1\n\ny = x // 0").unwrap_err();
        assert_eq!(failure.line, 3);
    }

    #[test]
    fn infinite_loops_do_not_block_the_game() {
        let program = compile_python("while True:\n    pass", &api()).unwrap();
        let mut vm = Vm::new(&program);
        assert_eq!(
            vm.run(&program, &mut TestHost::default(), 50),
            Step::OutOfFuel
        );
        assert!(matches!(vm.current_line(&program), Some(1 | 2)));
    }

    #[test]
    fn suspension_reports_the_waiting_line() {
        let program = compile_python("x = 1\nattendre(2)\nprint(x)", &api()).unwrap();
        let mut vm = Vm::new(&program);
        let mut host = TestHost::default();
        assert_eq!(vm.run(&program, &mut host, 100), Step::Suspended);
        assert_eq!(vm.current_line(&program), Some(2));
        vm.resume(Value::None);
        assert_eq!(vm.run(&program, &mut host, 100), Step::Finished);
        assert_eq!(host.output, vec!["1"]);
    }
}
