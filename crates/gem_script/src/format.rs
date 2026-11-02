//! Affichage des valeurs à la manière de Rust (`{}` et `{:?}`), guidé par une description
//! du type calculée par le compilateur Rust.
//!
//! La machine virtuelle ne connaît que des valeurs simples (`Some(x)` y est représenté par
//! `x`, une structure par une liste…) : la description dit comment les relire.
//!
//! Descriptions (des [`Value`]) :
//! - `"i"` entier, `"b"` booléen, `"s"` texte, `"u"` unité `()`, `"?"` inconnu ;
//! - `("t", d1, d2…)` tuple, `("v", d)` vecteur, `("o", d)` `Option`,
//!   `("res", d_ok, d_err)` `Result` (valeur : `(true, x)` ou `(false, e)`) ;
//! - `("S", nom, (champ, d)…)` structure (valeur : liste ou tuple des champs) ;
//! - `("E", nom, (variante, d…)…)` énumération (valeur : `(variante, données…)`) ;
//! - `("e", nom0, nom1…)` énumération du jeu (valeur : un entier, indice du nom).

use crate::value::Value;

pub fn format_value(value: &Value, description: &Value, debug: bool) -> String {
    let mut out = String::new();
    write(&mut out, value, description, debug);
    out
}

fn write(out: &mut String, value: &Value, description: &Value, debug: bool) {
    match (description, value) {
        (Value::Str(kind), _) => match (kind.as_str(), value) {
            (_, Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
            ("s", Value::Str(s)) if debug => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        other => out.push(other),
                    }
                }
                out.push('"');
            }
            ("u", _) => out.push_str("()"),
            (_, Value::Range(start, stop, _)) => out.push_str(&format!("{start}..{stop}")),
            (_, Value::Tuple(items)) => {
                let unknown = Value::Str("?".into());
                write_seq(out, "(", ")", items.iter().map(|v| (v, &unknown)), debug);
            }
            (_, Value::List(items)) => {
                let unknown = Value::Str("?".into());
                write_seq(out, "[", "]", items.iter().map(|v| (v, &unknown)), debug);
            }
            (_, other) => out.push_str(&other.to_string()),
        },
        (Value::Tuple(parts), _) => {
            let kind = match parts.first() {
                Some(Value::Str(kind)) => kind.as_str(),
                _ => "",
            };
            let rest = &parts[1.min(parts.len())..];
            match (kind, value) {
                ("t", Value::Tuple(items) | Value::List(items)) => {
                    write_seq(out, "(", ")", items.iter().zip(rest), debug);
                    if items.len() == 1 {
                        out.insert(out.len() - 1, ',');
                    }
                }
                ("v", Value::List(items) | Value::Tuple(items)) => {
                    let inner = rest.first().cloned().unwrap_or(Value::Str("?".into()));
                    write_seq(out, "[", "]", items.iter().map(|v| (v, &inner)), debug);
                }
                ("o", Value::None) => out.push_str("None"),
                ("o", inner_value) => {
                    out.push_str("Some(");
                    write(out, inner_value, rest.first().unwrap_or(&Value::None), true);
                    out.push(')');
                }
                ("res", Value::Tuple(pair)) if pair.len() == 2 => {
                    let ok = pair[0].truthy();
                    out.push_str(if ok { "Ok(" } else { "Err(" });
                    let inner = rest.get(if ok { 0 } else { 1 }).unwrap_or(&Value::None);
                    write(out, &pair[1], inner, true);
                    out.push(')');
                }
                ("S", Value::List(fields) | Value::Tuple(fields)) => {
                    if let Some(Value::Str(name)) = rest.first() {
                        out.push_str(name);
                    }
                    out.push_str(" { ");
                    for (i, (field, spec)) in
                        fields.iter().zip(&rest[1.min(rest.len())..]).enumerate()
                    {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        if let Value::Tuple(spec) = spec
                            && let [Value::Str(field_name), field_type] = spec.as_slice()
                        {
                            out.push_str(field_name);
                            out.push_str(": ");
                            write(out, field, field_type, true);
                        }
                    }
                    out.push_str(" }");
                }
                ("E", Value::Tuple(items)) => {
                    let Some(Value::Str(variant)) = items.first() else {
                        out.push_str(&value.to_string());
                        return;
                    };
                    out.push_str(variant);
                    let payload = &items[1..];
                    if payload.is_empty() {
                        return;
                    }
                    let spec = rest[1.min(rest.len())..].iter().find_map(|v| match v {
                        Value::Tuple(spec)
                            if spec.first() == Some(&Value::Str(variant.clone())) =>
                        {
                            Some(&spec[1..])
                        }
                        _ => None,
                    });
                    let spec = spec.unwrap_or(&[]);
                    let unknown = Value::Str("?".into());
                    write_seq(
                        out,
                        "(",
                        ")",
                        payload
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (v, spec.get(i).unwrap_or(&unknown))),
                        true,
                    );
                }
                ("e", Value::Int(index)) => match rest.get(*index as usize) {
                    Some(Value::Str(name)) => out.push_str(name),
                    _ => out.push_str(&index.to_string()),
                },
                _ => write(out, value, &Value::Str("?".into()), debug),
            }
        }
        _ => out.push_str(&value.to_string()),
    }
}

fn write_seq<'a>(
    out: &mut String,
    open: &str,
    close: &str,
    items: impl Iterator<Item = (&'a Value, &'a Value)>,
    _debug: bool,
) {
    out.push_str(open);
    for (i, (value, description)) in items.enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        // Dans une collection, Rust affiche toujours les éléments en mode debug.
        write(out, value, description, true);
    }
    out.push_str(close);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Value {
        Value::Str(text.into())
    }

    #[test]
    fn rust_style_display() {
        assert_eq!(format_value(&Value::Bool(true), &s("b"), false), "true");
        assert_eq!(format_value(&s("salut"), &s("s"), false), "salut");
        assert_eq!(format_value(&s("salut"), &s("s"), true), "\"salut\"");
        let option = Value::Tuple(vec![s("o"), s("i")]);
        assert_eq!(format_value(&Value::Int(3), &option, true), "Some(3)");
        assert_eq!(format_value(&Value::None, &option, true), "None");
        let position = Value::Tuple(vec![s("t"), s("i"), s("i")]);
        let list = Value::Tuple(vec![s("v"), position]);
        let value = Value::List(vec![Value::Tuple(vec![Value::Int(1), Value::Int(2)])]);
        assert_eq!(format_value(&value, &list, true), "[(1, 2)]");
        let point = Value::Tuple(vec![
            s("S"),
            s("Point"),
            Value::Tuple(vec![s("x"), s("i")]),
            Value::Tuple(vec![s("y"), s("i")]),
        ]);
        let value = Value::List(vec![Value::Int(1), Value::Int(-2)]);
        assert_eq!(format_value(&value, &point, true), "Point { x: 1, y: -2 }");
        let mode = Value::Tuple(vec![
            s("E"),
            s("Mode"),
            Value::Tuple(vec![s("Aller"), position_description()]),
        ]);
        let value = Value::Tuple(vec![
            s("Aller"),
            Value::Tuple(vec![Value::Int(3), Value::Int(4)]),
        ]);
        assert_eq!(format_value(&value, &mode, true), "Aller((3, 4))");
        let result = Value::Tuple(vec![s("res"), s("u"), s("i")]);
        let ok = Value::Tuple(vec![Value::Bool(true), Value::Tuple(vec![])]);
        assert_eq!(format_value(&ok, &result, true), "Ok(())");
    }

    fn position_description() -> Value {
        Value::Tuple(vec![s("t"), s("i"), s("i")])
    }
}
