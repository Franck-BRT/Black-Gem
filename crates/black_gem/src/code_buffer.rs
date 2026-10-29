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
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
    }

    /// Entrée : nouvelle ligne, avec la même indentation (et un cran de plus après « : »).
    pub fn newline(&mut self) {
        let line = &self.text[self.line_start()..self.cursor];
        let mut indent = line.len() - line.trim_start_matches(' ').len();
        if line.trim_end().ends_with(':') {
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
