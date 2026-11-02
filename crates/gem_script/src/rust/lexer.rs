//! Découpe un programme Rust en jetons.

use crate::error::{ErrorKind, ScriptError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Str(String),
    Keyword(&'static str),
    Punct(&'static str),
    /// `println!`, `vec!`… (le nom sans le point d'exclamation).
    Macro(String),
    /// `'a`, `'static` (durées de vie : acceptées dans les types, sans effet).
    Lifetime(String),
    Eof,
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(name) => name.clone(),
            Tok::Int(n) => n.to_string(),
            Tok::Str(s) => format!("\"{s}\""),
            Tok::Keyword(k) | Tok::Punct(k) => (*k).to_string(),
            Tok::Macro(name) => format!("{name}!"),
            Tok::Lifetime(name) => format!("'{name}"),
            Tok::Eof => "fin du programme / end of program".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

const KEYWORDS: [&str; 35] = [
    "as", "break", "const", "continue", "else", "enum", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mut", "pub", "ref", "return", "self", "Self", "struct", "true", "use",
    "while", "where", "trait", "type", "static", "mod", "dyn", "move", "unsafe", "async", "crate",
];

/// Ponctuation, de la plus longue à la plus courte.
const PUNCTUATION: [&str; 40] = [
    "..=", "...", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=",
    "%=", "..", "{", "}", "(", ")", "[", "]", ";", ",", ":", ".", "=", "<", ">", "+", "-", "*",
    "/", "%", "&", "|", "!", "?", "#",
];

fn rust_error(line: u32, french: impl Into<String>, english: impl Into<String>) -> ScriptError {
    ScriptError::new(
        line,
        ErrorKind::Rust {
            code: "",
            french: french.into(),
            english: english.into(),
        },
    )
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, ScriptError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut line = 1u32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Commentaires.
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = line;
            let mut depth = 0;
            loop {
                if i + 1 >= chars.len() {
                    return Err(rust_error(
                        start,
                        "commentaire /* non terminé : il manque */",
                        "unterminated /* comment: */ is missing",
                    ));
                }
                if chars[i] == '/' && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            continue;
        }
        let push = |tokens: &mut Vec<Token>, tok| tokens.push(Token { tok, line });

        if c.is_ascii_digit() {
            let after_dot = matches!(
                tokens.last(),
                Some(Token {
                    tok: Tok::Punct("."),
                    ..
                })
            );
            let start = i;
            let radix = if c == '0' && matches!(chars.get(i + 1), Some('x' | 'b' | 'o')) {
                i += 2;
                match chars[i - 1] {
                    'x' => 16,
                    'b' => 2,
                    _ => 8,
                }
            } else {
                10
            };
            let digits_start = i;
            while i < chars.len() && (chars[i].is_digit(radix) || chars[i] == '_') {
                i += 1;
            }
            // Un indice de tuple (t.0) s'arrête au point : « t.0.1 ».
            if !after_dot
                && radix == 10
                && i + 1 < chars.len()
                && chars[i] == '.'
                && chars[i + 1].is_ascii_digit()
            {
                return Err(ScriptError::new(line, ErrorKind::DecimalNumber));
            }
            let text: String = chars[digits_start..i]
                .iter()
                .filter(|c| **c != '_')
                .collect();
            // Suffixe de type (10i64, 3usize…) : tous les entiers sont des i64 dans le jeu.
            let suffix_start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric()) {
                i += 1;
            }
            let suffix: String = chars[suffix_start..i].iter().collect();
            if suffix.starts_with('f') {
                return Err(ScriptError::new(line, ErrorKind::DecimalNumber));
            }
            if !suffix.is_empty()
                && !matches!(
                    suffix.as_str(),
                    "i8" | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "usize"
                )
            {
                return Err(ScriptError::new(
                    line,
                    ErrorKind::UnexpectedToken(chars[start..i].iter().collect()),
                ));
            }
            match i64::from_str_radix(&text, radix) {
                Ok(n) => push(&mut tokens, Tok::Int(n)),
                Err(_) => return Err(ScriptError::new(line, ErrorKind::NumberTooBig)),
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if chars.get(i) == Some(&'!') && chars.get(i + 1) != Some(&'=') {
                i += 1;
                push(&mut tokens, Tok::Macro(word));
            } else if let Some(k) = KEYWORDS.iter().find(|k| **k == word) {
                push(&mut tokens, Tok::Keyword(k));
            } else {
                push(&mut tokens, Tok::Ident(word));
            }
            continue;
        }
        if c == '"' {
            let start = line;
            i += 1;
            let mut text = String::new();
            loop {
                let Some(&ch) = chars.get(i) else {
                    return Err(ScriptError::new(start, ErrorKind::UnterminatedString));
                };
                i += 1;
                match ch {
                    '"' => break,
                    '\\' => {
                        let escaped = chars.get(i).copied();
                        i += 1;
                        match escaped {
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            Some('0') => text.push('\0'),
                            Some('\\') => text.push('\\'),
                            Some('"') => text.push('"'),
                            Some('\'') => text.push('\''),
                            Some('\n') => {
                                // Ligne continuée : les espaces du début sont ignorés.
                                line += 1;
                                while chars.get(i).is_some_and(|c| c.is_whitespace()) {
                                    if chars[i] == '\n' {
                                        line += 1;
                                    }
                                    i += 1;
                                }
                            }
                            _ => {
                                return Err(rust_error(
                                    line,
                                    "échappement inconnu dans le texte (\\n, \\t, \\\\ et \\\" sont permis)",
                                    "unknown escape in string (\\n, \\t, \\\\ and \\\" are allowed)",
                                ));
                            }
                        }
                    }
                    '\n' => {
                        line += 1;
                        text.push('\n');
                    }
                    other => text.push(other),
                }
            }
            tokens.push(Token {
                tok: Tok::Str(text),
                line: start,
            });
            continue;
        }
        if c == '\'' {
            // Caractère 'a' (traité comme un texte d'un caractère).
            if chars.get(i + 2) == Some(&'\'') && chars.get(i + 1) != Some(&'\\') {
                push(&mut tokens, Tok::Str(chars[i + 1].to_string()));
                i += 3;
                continue;
            }
            if chars
                .get(i + 1)
                .is_some_and(|c| c.is_alphabetic() || *c == '_')
            {
                let start = i + 1;
                i += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                push(&mut tokens, Tok::Lifetime(chars[start..i].iter().collect()));
                continue;
            }
            return Err(ScriptError::new(line, ErrorKind::UnexpectedChar(c)));
        }
        let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
        match PUNCTUATION.iter().find(|p| rest.starts_with(**p)) {
            Some(p) => {
                push(&mut tokens, Tok::Punct(p));
                i += p.chars().count();
            }
            None => return Err(ScriptError::new(line, ErrorKind::UnexpectedChar(c))),
        }
    }
    tokens.push(Token {
        tok: Tok::Eof,
        line,
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
    fn tokens() {
        assert_eq!(
            toks("let mut x = 0..=10; // commentaire\nprintln!(\"{}\", t.0.1);"),
            vec![
                Tok::Keyword("let"),
                Tok::Keyword("mut"),
                Tok::Ident("x".into()),
                Tok::Punct("="),
                Tok::Int(0),
                Tok::Punct("..="),
                Tok::Int(10),
                Tok::Punct(";"),
                Tok::Macro("println".into()),
                Tok::Punct("("),
                Tok::Str("{}".into()),
                Tok::Punct(","),
                Tok::Ident("t".into()),
                Tok::Punct("."),
                Tok::Int(0),
                Tok::Punct("."),
                Tok::Int(1),
                Tok::Punct(")"),
                Tok::Punct(";"),
                Tok::Eof,
            ]
        );
        assert_eq!(toks("1_000u32 0x1F x != y")[0], Tok::Int(1000));
        assert_eq!(toks("0x1F")[0], Tok::Int(31));
        assert!(tokenize("let x = 1.5;").is_err());
    }
}
