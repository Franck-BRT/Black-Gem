//! Erreurs de script, avec des messages pédagogiques en français et en anglais.

use std::fmt;

/// Langue des messages d'erreur.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    French,
    English,
}

/// Une erreur, à la compilation ou à l'exécution, et la ligne où elle se produit (à partir de 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub line: u32,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    // Écriture du programme.
    UnexpectedChar(char),
    UnterminatedString,
    TabIndent,
    BadIndent,
    UnexpectedIndent,
    ExpectedIndent,
    Expected(&'static str),
    UnexpectedToken(String),
    NumberTooBig,
    DecimalNumber,
    InvalidTarget,
    UnknownName {
        name: String,
        suggestion: Option<String>,
    },
    NotAValue(String),
    ReturnOutsideFunction,
    LoopControlOutsideLoop(&'static str),
    FloatDivision,
    DuplicateParameter(String),
    UnsupportedMethod(String),
    // Exécution.
    UndefinedVariable(String),
    TypeMismatch {
        op: &'static str,
        left: &'static str,
        right: &'static str,
    },
    NotCallable(&'static str),
    WrongArgCount {
        name: String,
        expected: String,
        got: usize,
    },
    WrongArgType {
        name: String,
        expected: &'static str,
        got: &'static str,
    },
    DivisionByZero,
    IndexOutOfRange {
        index: i64,
        len: usize,
    },
    NotIndexable(&'static str),
    NotIterable(&'static str),
    UnpackMismatch {
        expected: usize,
        got: usize,
    },
    NegativePower,
    Overflow,
    RecursionTooDeep,
    TooBig,
    /// Erreur signalée par le jeu (par exemple : « il n'y a pas de tour ici »).
    Game {
        french: String,
        english: String,
    },
}

impl ScriptError {
    pub fn new(line: u32, kind: ErrorKind) -> Self {
        Self { line, kind }
    }

    /// Message complet : « Ligne 3 : ... ».
    pub fn message(&self, lang: Lang) -> String {
        match lang {
            Lang::French => format!("Ligne {} : {}", self.line, self.kind.message(lang)),
            Lang::English => format!("Line {}: {}", self.line, self.kind.message(lang)),
        }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Lang::English))
    }
}

impl ErrorKind {
    pub fn message(&self, lang: Lang) -> String {
        use ErrorKind::*;
        let fr = lang == Lang::French;
        match self {
            UnexpectedChar(c) if fr => format!("le caractère « {c} » n'est pas compris ici"),
            UnexpectedChar(c) => format!("the character '{c}' is not understood here"),
            UnterminatedString if fr => "texte non terminé : il manque le guillemet de fin".into(),
            UnterminatedString => "unterminated string: the closing quote is missing".into(),
            TabIndent if fr => "utilise des espaces (pas de tabulation) pour indenter".into(),
            TabIndent => "use spaces (not tabs) for indentation".into(),
            BadIndent if fr => "l'indentation ne correspond à aucun bloc ouvert au-dessus".into(),
            BadIndent => "this indentation does not match any block above".into(),
            UnexpectedIndent if fr => {
                "indentation inattendue : cette ligne est décalée sans raison".into()
            }
            UnexpectedIndent => "unexpected indent: this line is shifted for no reason".into(),
            ExpectedIndent if fr => {
                "un bloc est attendu : décale les lignes qui suivent « : » de 4 espaces".into()
            }
            ExpectedIndent => "a block is expected: indent the lines after ':' by 4 spaces".into(),
            Expected(what) if fr => format!("« {what} » est attendu ici"),
            Expected(what) => format!("'{what}' is expected here"),
            UnexpectedToken(token) if fr => format!("« {token} » n'est pas attendu ici"),
            UnexpectedToken(token) => format!("'{token}' is not expected here"),
            NumberTooBig if fr => "nombre trop grand".into(),
            NumberTooBig => "number too big".into(),
            DecimalNumber if fr => {
                "les nombres à virgule n'existent pas dans le jeu : utilise des nombres entiers"
                    .into()
            }
            DecimalNumber => "decimal numbers do not exist in the game: use whole numbers".into(),
            InvalidTarget if fr => {
                "on ne peut donner une valeur qu'à une variable (ou à un élément de liste)".into()
            }
            InvalidTarget => "only a variable (or a list item) can be assigned".into(),
            UnknownName { name, suggestion } => {
                let base = if fr {
                    format!("le nom « {name} » est inconnu")
                } else {
                    format!("the name '{name}' is unknown")
                };
                match (suggestion, fr) {
                    (Some(s), true) => format!("{base} — voulais-tu écrire « {s} » ?"),
                    (Some(s), false) => format!("{base} — did you mean '{s}'?"),
                    (None, _) => base,
                }
            }
            NotAValue(name) if fr => {
                format!(
                    "« {name} » est une fonction : appelle-la avec des parenthèses, comme {name}()"
                )
            }
            NotAValue(name) => {
                format!("'{name}' is a function: call it with parentheses, like {name}()")
            }
            ReturnOutsideFunction if fr => {
                "« return » ne s'utilise que dans une fonction (def)".into()
            }
            ReturnOutsideFunction => "'return' can only be used inside a function (def)".into(),
            LoopControlOutsideLoop(word) if fr => {
                format!("« {word} » ne s'utilise que dans une boucle (while ou for)")
            }
            LoopControlOutsideLoop(word) => {
                format!("'{word}' can only be used inside a loop (while or for)")
            }
            FloatDivision if fr => {
                "les nombres à virgule n'existent pas dans le jeu : utilise // (division entière)"
                    .into()
            }
            FloatDivision => {
                "decimal numbers do not exist in the game: use // (integer division)".into()
            }
            DuplicateParameter(name) if fr => format!("le paramètre « {name} » apparaît deux fois"),
            DuplicateParameter(name) => format!("the parameter '{name}' appears twice"),
            UnsupportedMethod(name) if fr => {
                format!("la méthode « .{name}() » n'existe pas (essaie .append(x))")
            }
            UnsupportedMethod(name) => {
                format!("the method '.{name}()' does not exist (try .append(x))")
            }
            UndefinedVariable(name) if fr => {
                format!("la variable « {name} » est utilisée avant d'avoir reçu une valeur")
            }
            UndefinedVariable(name) => {
                format!("the variable '{name}' is used before being given a value")
            }
            TypeMismatch { op, left, right } if fr => format!(
                "impossible de faire {} {op} {}",
                type_name(left, lang),
                type_name(right, lang)
            ),
            TypeMismatch { op, left, right } => format!(
                "cannot compute {} {op} {}",
                type_name(left, lang),
                type_name(right, lang)
            ),
            NotCallable(kind) if fr => format!(
                "{} ne peut pas être appelé comme une fonction",
                type_name(kind, lang)
            ),
            NotCallable(kind) => {
                format!("{} cannot be called like a function", type_name(kind, lang))
            }
            WrongArgCount {
                name,
                expected,
                got,
            } if fr => format!("{name}() attend {expected} argument(s), mais en reçoit {got}"),
            WrongArgCount {
                name,
                expected,
                got,
            } => format!("{name}() takes {expected} argument(s) but got {got}"),
            WrongArgType {
                name,
                expected,
                got,
            } => {
                if fr {
                    format!(
                        "{name}() attend {}, pas {}",
                        type_name(expected, lang),
                        type_name(got, lang)
                    )
                } else {
                    format!(
                        "{name}() expects {}, not {}",
                        type_name(expected, lang),
                        type_name(got, lang)
                    )
                }
            }
            DivisionByZero if fr => "division par zéro".into(),
            DivisionByZero => "division by zero".into(),
            IndexOutOfRange { index, len } if fr => format!(
                "l'indice {index} est en dehors de la liste (elle contient {len} élément(s), de 0 à {})",
                *len as i64 - 1
            ),
            IndexOutOfRange { index, len } => format!(
                "index {index} is outside the list (it has {len} item(s), from 0 to {})",
                *len as i64 - 1
            ),
            NotIndexable(kind) if fr => format!(
                "on ne peut pas prendre un élément [ ] d'{}",
                type_name(kind, lang)
            ),
            NotIndexable(kind) => format!("cannot take an item [ ] from {}", type_name(kind, lang)),
            NotIterable(kind) if fr => format!(
                "on ne peut pas parcourir {} avec « for »",
                type_name(kind, lang)
            ),
            NotIterable(kind) => format!("cannot loop over {} with 'for'", type_name(kind, lang)),
            UnpackMismatch { expected, got } if fr => {
                format!("{expected} variable(s) à remplir, mais {got} valeur(s)")
            }
            UnpackMismatch { expected, got } => {
                format!("{expected} variable(s) to fill, but {got} value(s)")
            }
            NegativePower if fr => "puissance négative impossible sans nombres à virgule".into(),
            NegativePower => "negative powers need decimal numbers, which do not exist here".into(),
            Overflow if fr => "nombre trop grand (dépassement)".into(),
            Overflow => "number too big (overflow)".into(),
            RecursionTooDeep if fr => {
                "trop d'appels de fonctions imbriqués (une fonction s'appelle-t-elle sans fin ?)"
                    .into()
            }
            RecursionTooDeep => {
                "too many nested calls (does a function call itself forever?)".into()
            }
            TooBig if fr => "liste ou texte trop grand".into(),
            TooBig => "list or text too big".into(),
            Game { french, english } => {
                if fr {
                    french.clone()
                } else {
                    english.clone()
                }
            }
        }
    }
}

/// Nom d'un type de valeur, avec son article (« un nombre », « a number »).
pub fn type_name(kind: &str, lang: Lang) -> &'static str {
    let fr = lang == Lang::French;
    match kind {
        "int" if fr => "un nombre",
        "int" => "a number",
        "bool" if fr => "un booléen (True/False)",
        "bool" => "a boolean (True/False)",
        "str" if fr => "un texte",
        "str" => "a text",
        "None" if fr => "None (rien)",
        "None" => "None (nothing)",
        "list" if fr => "une liste",
        "list" => "a list",
        "tuple" if fr => "un tuple",
        "tuple" => "a tuple",
        "range" if fr => "un range",
        "range" => "a range",
        "function" if fr => "une fonction",
        "function" => "a function",
        "position" if fr => "une position (x, y)",
        "position" => "a position (x, y)",
        _ if fr => "une valeur",
        _ => "a value",
    }
}

/// Distance d'édition (pour suggérer le nom le plus proche d'une faute de frappe).
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_in_both_languages() {
        let error = ScriptError::new(
            3,
            ErrorKind::UnknownName {
                name: "aler_vers".into(),
                suggestion: Some("aller_vers".into()),
            },
        );
        assert_eq!(
            error.message(Lang::French),
            "Ligne 3 : le nom « aler_vers » est inconnu — voulais-tu écrire « aller_vers » ?"
        );
        assert_eq!(
            error.message(Lang::English),
            "Line 3: the name 'aler_vers' is unknown — did you mean 'aller_vers'?"
        );
    }

    #[test]
    fn edit_distance_counts_typos() {
        assert_eq!(edit_distance("aler_vers", "aller_vers"), 1);
        assert_eq!(edit_distance("tours", "tours"), 0);
        assert_eq!(edit_distance("abc", "xyz"), 3);
    }
}
