//! Texte en cours d'édition et position du curseur, avec les règles d'édition de code :
//! indentation automatique, tabulation à 4 espaces, retour arrière qui désindente.

const INDENT: usize = 4;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CodeBuffer {
    pub text: String,
    /// Position du curseur, en octets (toujours sur une frontière de caractère).
    pub cursor: usize,
}

impl CodeBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { text, cursor: 0 }
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    /// Ligne (à partir de 0) et colonne (en caractères) du curseur.
    pub fn line_col(&self) -> (usize, usize) {
        let line = self.text[..self.cursor].matches('\n').count();
        let col = self.text[self.line_start()..self.cursor].chars().count();
        (line, col)
    }

    pub fn insert(&mut self, s: &str) {
        // Les retours chariot (collage depuis Windows) et tabulations sont normalisés.
        let s = s
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ");
        // Rust : « } » tapé en début de ligne revient d'un cran.
        if s == "}" {
            let start = self.line_start();
            let before = &self.text[start..self.cursor];
            if before.len() >= INDENT && before.chars().all(|c| c == ' ') {
                self.text
                    .replace_range(self.cursor - INDENT..self.cursor, "");
                self.cursor -= INDENT;
            }
        }
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
    }

    /// Entrée : nouvelle ligne, avec la même indentation (et un cran de plus après « : »
    /// en Python, ou après « { » en Rust).
    pub fn newline(&mut self) {
        let line = &self.text[self.line_start()..self.cursor];
        let mut indent = line.len() - line.trim_start_matches(' ').len();
        if line.trim_end().ends_with([':', '{', '(', '[']) {
            indent += INDENT;
        }
        self.insert(&format!("\n{}", " ".repeat(indent)));
    }

    /// Tabulation : espaces jusqu'au prochain multiple de 4.
    pub fn tab(&mut self) {
        let (_, col) = self.line_col();
        self.insert(&" ".repeat(INDENT - col % INDENT));
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[self.line_start()..self.cursor];
        // Dans l'indentation : on retire un cran entier.
        let remove = if !before.is_empty() && before.chars().all(|c| c == ' ') {
            let width = before.len();
            let target = (width - 1) / INDENT * INDENT;
            width - target
        } else {
            self.text[..self.cursor]
                .chars()
                .next_back()
                .map_or(0, char::len_utf8)
        };
        self.text
            .replace_range(self.cursor - remove..self.cursor, "");
        self.cursor -= remove;
    }

    pub fn delete(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.text
                .replace_range(self.cursor..self.cursor + c.len_utf8(), "");
        }
    }

    pub fn left(&mut self) {
        if let Some(c) = self.text[..self.cursor].chars().next_back() {
            self.cursor -= c.len_utf8();
        }
    }

    pub fn right(&mut self) {
        if let Some(c) = self.text[self.cursor..].chars().next() {
            self.cursor += c.len_utf8();
        }
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
    }

    pub fn up(&mut self) {
        let (_, col) = self.line_col();
        let start = self.line_start();
        if start == 0 {
            self.cursor = 0;
            return;
        }
        self.cursor = start - 1;
        self.home();
        self.move_to_column(col);
    }

    pub fn down(&mut self) {
        let (_, col) = self.line_col();
        let end = self.line_end();
        if end == self.text.len() {
            self.cursor = end;
            return;
        }
        self.cursor = end + 1;
        self.move_to_column(col);
    }

    fn move_to_column(&mut self, col: usize) {
        let end = self.line_end();
        let line = &self.text[self.cursor..end];
        self.cursor += line.char_indices().nth(col).map_or(line.len(), |(i, _)| i);
    }

    // ----- Édition par lignes (éditeur par blocs, au doigt) -----

    fn lines(&self) -> Vec<String> {
        self.text.split('\n').map(str::to_string).collect()
    }

    /// Remplace tout le texte et place le curseur en fin de la ligne `line`.
    fn set_lines(&mut self, lines: Vec<String>, line: usize) {
        self.text = lines.join("\n");
        self.cursor_to_line(line);
        self.end();
    }

    /// Place le curseur au bout d'une ligne (touchée du doigt).
    pub fn cursor_to_line(&mut self, line: usize) {
        let mut offset = 0;
        for (i, l) in self.text.split('\n').enumerate() {
            if i == line {
                self.cursor = offset + l.len();
                return;
            }
            offset += l.len() + 1;
        }
        self.cursor = self.text.len();
    }

    fn indent_of(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    /// Ajoute une instruction sous la ligne du curseur, à la bonne indentation (une ligne
    /// vide est remplacée). `opens` : l'instruction ouvre un bloc (`while …:` ou `loop {`) ;
    /// `closing` : la ligne qui le ferme (Rust : `}`).
    pub fn insert_statement(&mut self, statement: &str, opens: bool, closing: Option<&str>) {
        let (line, _) = self.line_col();
        let mut lines = self.lines();
        let current = lines[line].clone();
        let blank = current.trim().is_empty();
        let indent = if blank {
            Self::indent_of(&current)
        } else {
            let opener = current.trim_end().ends_with([':', '{']);
            Self::indent_of(&current) + if opener { INDENT } else { 0 }
        };
        let pad = " ".repeat(indent);
        let mut new_lines = vec![format!("{pad}{statement}")];
        let mut cursor_line = line + usize::from(!blank);
        if opens {
            new_lines.push(" ".repeat(indent + INDENT));
            cursor_line += 1;
            if let Some(close) = closing {
                new_lines.push(format!("{pad}{close}"));
            }
        }
        let at = if blank { line } else { line + 1 };
        if blank {
            lines.remove(line);
        }
        for (k, l) in new_lines.into_iter().enumerate() {
            lines.insert(at + k, l);
        }
        self.set_lines(lines, cursor_line);
    }

    /// Décale la ligne du curseur d'un cran (vers la droite, ou la gauche).
    pub fn shift_line(&mut self, right: bool) {
        let (line, _) = self.line_col();
        let mut lines = self.lines();
        let current = &mut lines[line];
        if right {
            current.insert_str(0, &" ".repeat(INDENT));
        } else {
            let remove = Self::indent_of(current).min(INDENT);
            current.drain(..remove);
        }
        self.set_lines(lines, line);
    }

    /// Échange la ligne du curseur avec celle du dessus (ou du dessous).
    pub fn move_line(&mut self, up: bool) {
        let (line, _) = self.line_col();
        let mut lines = self.lines();
        let other = if up {
            line.checked_sub(1)
        } else {
            (line + 1 < lines.len()).then_some(line + 1)
        };
        if let Some(other) = other {
            lines.swap(line, other);
            self.set_lines(lines, other);
        }
    }

    pub fn delete_line(&mut self) {
        let (line, _) = self.line_col();
        let mut lines = self.lines();
        if lines.len() == 1 {
            lines[0].clear();
        } else {
            lines.remove(line);
        }
        let line = line.min(lines.len() - 1);
        self.set_lines(lines, line);
    }

    /// Ajoute `delta` au dernier nombre de la ligne du curseur (`munitions(x, y) < 12` → 13).
    pub fn bump_number(&mut self, delta: i64) {
        let (line, _) = self.line_col();
        let mut lines = self.lines();
        let current = &lines[line];
        let bytes = current.as_bytes();
        let Some(end) = bytes.iter().rposition(u8::is_ascii_digit) else {
            return;
        };
        let mut start = end;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        let negative = start > 0 && bytes[start - 1] == b'-';
        let first = if negative { start - 1 } else { start };
        let Ok(value) = current[first..=end].parse::<i64>() else {
            return;
        };
        let next = value.saturating_add(delta);
        let replaced = format!("{}{next}{}", &current[..first], &current[end + 1..]);
        lines[line] = replaced;
        self.set_lines(lines, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> CodeBuffer {
        let mut b = CodeBuffer::new(text);
        b.cursor = text.len();
        b
    }

    #[test]
    fn newline_keeps_and_increases_indentation() {
        let mut b = buffer("while True:");
        b.newline();
        b.insert("x = 1");
        b.newline();
        assert_eq!(b.text, "while True:\n    x = 1\n    ");
    }

    #[test]
    fn block_editing_by_lines() {
        let mut b = buffer("while True:");
        b.insert_statement("for x, y in tours():", true, None);
        b.insert_statement("aller_vers(x, y)", false, None);
        assert_eq!(
            b.text,
            "while True:\n    for x, y in tours():\n        aller_vers(x, y)"
        );
        b.insert_statement("attendre(1)", false, None);
        b.shift_line(false);
        assert_eq!(b.text.lines().last(), Some("    attendre(1)"));
        b.bump_number(2);
        assert_eq!(b.text.lines().last(), Some("    attendre(3)"));
        b.move_line(true);
        assert_eq!(b.text.lines().nth(2), Some("    attendre(3)"));
        b.delete_line();
        assert_eq!(
            b.text,
            "while True:\n    for x, y in tours():\n        aller_vers(x, y)"
        );
        let mut r = buffer("fn main() {");
        r.insert_statement("loop {", true, Some("}"));
        r.insert_statement("attendre(1);", false, None);
        assert_eq!(
            r.text,
            "fn main() {\n    loop {\n        attendre(1);\n    }"
        );
        r.cursor_to_line(0);
        assert_eq!(r.line_col(), (0, 11));
    }

    #[test]
    fn rust_braces_indent_and_dedent() {
        let mut b = buffer("fn main() {");
        b.newline();
        b.insert("loop {");
        b.newline();
        b.insert("}");
        assert_eq!(b.text, "fn main() {\n    loop {\n    }");
    }

    #[test]
    fn backspace_removes_a_whole_indent_level() {
        let mut b = buffer("if x:\n        ");
        b.backspace();
        assert_eq!(b.text, "if x:\n    ");
        b.backspace();
        assert_eq!(b.text, "if x:\n");
        b.backspace();
        assert_eq!(b.text, "if x:");
    }

    #[test]
    fn accents_and_arrows() {
        let mut b = buffer("été\nà");
        b.up();
        assert_eq!(b.line_col(), (0, 1));
        b.left();
        b.delete();
        assert_eq!(b.text, "té\nà");
        b.end();
        b.down();
        assert_eq!(b.line_col(), (1, 1));
        b.backspace();
        assert_eq!(b.text, "té\n");
    }

    #[test]
    fn tab_and_paste_normalization() {
        let mut b = buffer("ab");
        b.tab();
        assert_eq!(b.text, "ab  ");
        b.insert("\tx\r\ny");
        assert_eq!(b.text, "ab      x\ny");
    }
}
