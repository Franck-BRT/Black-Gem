//! Valeurs manipulées par les programmes.
//!
//! Les listes ont une sémantique de *valeur* (copie à l'affectation) : plus simple à
//! expliquer aux débutants, et l'état complet reste copiable et déterministe.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Value {
    #[default]
    None,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Range(i64, i64, i64),
    /// Fonction définie par `def` (indice dans le programme).
    Function(u32),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "None",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Str(_) => "str",
            Value::List(_) => "list",
            Value::Tuple(_) => "tuple",
            Value::Range(..) => "range",
            Value::Function(_) => "function",
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::List(items) | Value::Tuple(items) => !items.is_empty(),
            Value::Range(start, stop, step) => range_len(*start, *stop, *step) > 0,
            Value::Function(_) => true,
        }
    }

    /// Valeur numérique (les booléens comptent comme 0 et 1, comme en Python).
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    /// Représentation dans une liste : les textes y sont entre guillemets.
    fn repr(&self) -> String {
        match self {
            Value::Str(s) => format!("'{s}'"),
            other => other.to_string(),
        }
    }
}

pub fn range_len(start: i64, stop: i64, step: i64) -> i64 {
    if step > 0 && start < stop {
        (stop - start + step - 1) / step
    } else if step < 0 && start > stop {
        (start - stop - step - 1) / -step
    } else {
        0
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => f.write_str("None"),
            Value::Bool(true) => f.write_str("True"),
            Value::Bool(false) => f.write_str("False"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => f.write_str(s),
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(Value::repr).collect();
                write!(f, "[{}]", inner.join(", "))
            }
            Value::Tuple(items) => {
                let inner: Vec<String> = items.iter().map(Value::repr).collect();
                if items.len() == 1 {
                    write!(f, "({},)", inner[0])
                } else {
                    write!(f, "({})", inner.join(", "))
                }
            }
            Value::Range(start, stop, 1) => write!(f, "range({start}, {stop})"),
            Value::Range(start, stop, step) => write!(f, "range({start}, {stop}, {step})"),
            Value::Function(_) => f.write_str("<fonction>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_like_python() {
        let v = Value::List(vec![
            Value::Int(1),
            Value::Str("a".into()),
            Value::Tuple(vec![Value::Int(2), Value::Int(3)]),
            Value::None,
        ]);
        assert_eq!(v.to_string(), "[1, 'a', (2, 3), None]");
        assert_eq!(Value::Str("salut".into()).to_string(), "salut");
    }

    #[test]
    fn range_lengths() {
        assert_eq!(range_len(0, 10, 1), 10);
        assert_eq!(range_len(0, 10, 3), 4);
        assert_eq!(range_len(10, 0, -2), 5);
        assert_eq!(range_len(5, 5, 1), 0);
    }
}
