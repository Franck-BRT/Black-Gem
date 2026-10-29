//! Découpe le texte du programme en jetons, en gérant l'indentation à la manière de Python.

use crate::error::{ErrorKind, ScriptError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Name(String),
    Int(i64),
    Str(String),
    Keyword(&'static str),
    Op(&'static str),
    Newline,
    Indent,
    Dedent,
    Eof,
}

impl Tok {
    /// Représentation lisible, pour les messages d'erreur.
    pub fn describe(&self) -> String {
        match self {
            Tok::Name(name) => name.clone(),
            Tok::Int(n) => n.to_string(),
            Tok::Str(s) => format!("\"{s}\""),
            Tok::Keyword(k) | Tok::Op(k) => (*k).to_string(),
            Tok::Newline => "fin de ligne / end of line".into(),
            Tok::Indent => "indentation".into(),
            Tok::Dedent => "fin de bloc / end of block".into(),
            Tok::Eof => "fin du programme / end of program".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

const KEYWORDS: [&str; 18] = [
    "if", "elif", "else", "while", "for", "in", "def", "return", "break", "continue", "pass",
    "and", "or", "not", "True", "False", "None", "global",
];

/// Opérateurs, du plus long au plus court (pour reconnaître « //= » avant « // » avant « / »).
const OPERATORS: [&str; 26] = [
    "//=", "**", "//", "==", "!=", "<=", ">=", "+=", "-=", "*=", "%=", "(", ")", "[", "]", ",",
    ":", ".", "=", "<", ">", "+", "-", "*", "/", "%",
];

pub fn tokenize(source: &str) -> Result<Vec<Token>, ScriptError> {
    let mut tokens = Vec::new();
    let mut indents = vec![0usize];
    let mut depth = 0usize; // parenthèses ouvertes : les retours à la ligne y sont ignorés
    let mut line_no = 0u32;

    for raw_line in source.lines() {
        line_no += 1;
        let err = |kind| Err(ScriptError::new(line_no, kind));
        let chars: Vec<char> = raw_line.chars().collect();
        let mut i = 0;

        if depth == 0 {
            // Début de ligne logique : mesure de l'indentation.
            let mut width = 0;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                if chars[i] == '\t' {
                    return err(ErrorKind::TabIndent);
                }
                width += 1;
                i += 1;
            }
            // Ligne vide ou commentaire seul : n'influence pas les blocs.
            if i == chars.len() || chars[i] == '#' {
                continue;
            }
            let current = *indents.last().unwrap();
            if width > current {
                indents.push(width);
                tokens.push(Token {
                    tok: Tok::Indent,
                    line: line_no,
                });
            } else {
                while width < *indents.last().unwrap() {
                    indents.pop();
                    tokens.push(Token {
                        tok: Tok::Dedent,
                        line: line_no,
                    });
                }
                if width != *indents.last().unwrap() {
                    return err(ErrorKind::BadIndent);
                }
            }
        }

        while i < chars.len() {
            let c = chars[i];
            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            if c == '#' {
                break;
            }
            let push = |tokens: &mut Vec<Token>, tok| {
                tokens.push(Token { tok, line: line_no });
            };
            if c.is_ascii_digit() {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    return err(ErrorKind::DecimalNumber);
                }
                let text: String = chars[start..i].iter().filter(|c| **c != '_').collect();
                match text.parse::<i64>() {
                    Ok(n) => push(&mut tokens, Tok::Int(n)),
                    Err(_) => return err(ErrorKind::NumberTooBig),
                }
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match KEYWORDS.iter().find(|k| **k == word) {
                    Some(k) => push(&mut tokens, Tok::Keyword(k)),
                    None => push(&mut tokens, Tok::Name(word)),
                }
                continue;
            }
            if c == '"' || c == '\'' {
                let quote = c;
                let mut text = String::new();
                i += 1;
                loop {
                    let Some(&ch) = chars.get(i) else {
                        return err(ErrorKind::UnterminatedString);
                    };
                    i += 1;
                    match ch {
                        _ if ch == quote => break,
                        '\\' => {
                            let escaped = chars.get(i).copied().unwrap_or('\\');
                            i += 1;
                            text.push(match escaped {
                                'n' => '\n',
                                't' => '\t',
                                other => other,
                            });
                        }
                        _ => text.push(ch),
                    }
                }
                push(&mut tokens, Tok::Str(text));
                continue;
            }
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            match OPERATORS.iter().find(|op| rest.starts_with(**op)) {
                Some(op) => {
                    match *op {
                        "(" | "[" => depth += 1,
                        ")" | "]" => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    push(&mut tokens, Tok::Op(op));
                    i += op.len();
                }
                None => return err(ErrorKind::UnexpectedChar(c)),
            }
        }
        if depth == 0 && tokens.last().is_some_and(|t| t.tok != Tok::Newline) {
            tokens.push(Token {
                tok: Tok::Newline,
                line: line_no,
            });
        }
    }

    let last_line = line_no.max(1);
    if tokens.last().is_some_and(|t| t.tok != Tok::Newline) {
        tokens.push(Token {
            tok: Tok::Newline,
            line: last_line,
        });
    }
    while indents.len() > 1 {
        indents.pop();
        tokens.push(Token {
            tok: Tok::Dedent,
            line: last_line,
        });
    }
    tokens.push(Token {
        tok: Tok::Eof,
        line: last_line,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(source: &str) -> Vec<Tok> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn indentation_produces_blocks() {
        let tokens = toks("while True:\n    x = 1\n\n    # commentaire\ny = 2\n");
        assert_eq!(
            tokens,
            vec![
                Tok::Keyword("while"),
                Tok::Keyword("True"),
                Tok::Op(":"),
                Tok::Newline,
                Tok::Indent,
                Tok::Name("x".into()),
                Tok::Op("="),
                Tok::Int(1),
                Tok::Newline,
                Tok::Dedent,
                Tok::Name("y".into()),
                Tok::Op("="),
                Tok::Int(2),
                Tok::Newline,
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn newlines_inside_brackets_are_ignored() {
        let tokens = toks("x = [1,\n     2]\n");
        assert!(!tokens[..tokens.len() - 2].contains(&Tok::Newline));
    }

    #[test]
    fn operators_strings_and_accents() {
        let tokens = toks("défi //= 'été\\n' # fin");
        assert_eq!(
            tokens,
            vec![
                Tok::Name("défi".into()),
                Tok::Op("//="),
                Tok::Str("été\n".into()),
                Tok::Newline,
                Tok::Eof
            ]
        );
    }

    #[test]
    fn helpful_errors() {
        assert_eq!(
            tokenize("x = 3.5").unwrap_err().kind,
            ErrorKind::DecimalNumber
        );
        assert_eq!(
            tokenize("if x:\n\ty = 1").unwrap_err().kind,
            ErrorKind::TabIndent
        );
        assert_eq!(
            tokenize("x = 'oups").unwrap_err().kind,
            ErrorKind::UnterminatedString
        );
        let bad = tokenize("if x:\n    y = 1\n  z = 2").unwrap_err();
        assert_eq!((bad.line, bad.kind), (3, ErrorKind::BadIndent));
        assert_eq!(
            tokenize("x = 1 $").unwrap_err().kind,
            ErrorKind::UnexpectedChar('$')
        );
    }
}
