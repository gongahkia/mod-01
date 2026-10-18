use serde::{Deserialize, Serialize};

use crate::{
    diagnostic::Diagnostic,
    span::{SourceFile, Span},
    token::{Token, TokenKind},
};

/// Tokens and recoverable lexical diagnostics for one module.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

struct Lexer<'source> {
    source: &'source SourceFile,
    bytes: &'source [u8],
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    indents: Vec<usize>,
}

/// Tokenizes ASCII MODL source, including structural newline/indent/dedent tokens.
#[must_use]
pub fn lex(source: &SourceFile) -> LexOutput {
    Lexer::new(source).run()
}

impl<'source> Lexer<'source> {
    fn new(source: &'source SourceFile) -> Self {
        Self {
            source,
            bytes: source.text().as_bytes(),
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            indents: vec![0],
        }
    }

    fn run(mut self) -> LexOutput {
        let mut line_start = 0;
        while line_start < self.bytes.len() {
            let newline = self.bytes[line_start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|relative| line_start + relative);
            let line_end = newline.unwrap_or(self.bytes.len());
            let content_end = if line_end > line_start && self.bytes[line_end - 1] == b'\r' {
                line_end - 1
            } else {
                line_end
            };
            self.lex_line(line_start, content_end);
            let newline_end = newline.map_or(line_end, |offset| offset + 1);
            self.push(TokenKind::Newline, content_end, newline_end);
            line_start = newline_end;
        }

        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(TokenKind::Dedent, self.bytes.len(), self.bytes.len());
        }
        self.push(TokenKind::Eof, self.bytes.len(), self.bytes.len());
        LexOutput {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn lex_line(&mut self, start: usize, end: usize) {
        let mut cursor = start;
        let mut width = 0;
        while cursor < end {
            match self.bytes[cursor] {
                b' ' => {
                    width += 1;
                    cursor += 1;
                }
                b'\t' => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M011002",
                            self.span(cursor, cursor + 1),
                            "tabs are not allowed in MODL source",
                        )
                        .with_primary_label("replace this tab with spaces"),
                    );
                    width = (width / 4 + 1) * 4;
                    cursor += 1;
                }
                _ => break,
            }
        }

        if cursor == end {
            return;
        }
        if self.bytes[cursor..end].starts_with(b"//") {
            self.lex_comment(cursor, end);
            return;
        }

        self.update_indent(width, start, cursor);
        while cursor < end {
            match self.bytes[cursor] {
                b' ' => cursor += 1,
                b'\t' => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M011002",
                            self.span(cursor, cursor + 1),
                            "tabs are not allowed in MODL source",
                        )
                        .with_primary_label("replace this tab with a space"),
                    );
                    cursor += 1;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    cursor = self.lex_identifier(cursor, end);
                }
                b'0'..=b'9' => cursor = self.lex_number(cursor, end),
                b'"' => cursor = self.lex_text(cursor, end),
                b'#' => cursor = self.lex_asset(cursor, end),
                b'/' if self.bytes[cursor..end].starts_with(b"//") => {
                    self.lex_comment(cursor, end);
                    break;
                }
                byte if !byte.is_ascii() => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M011001",
                            self.span(cursor, cursor + 1),
                            "MODL/1 source must contain ASCII characters only",
                        )
                        .with_primary_label("non-ASCII byte starts here"),
                    );
                    cursor += 1;
                }
                _ => cursor = self.lex_symbol(cursor, end),
            }
        }
    }

    fn update_indent(&mut self, width: usize, start: usize, cursor: usize) {
        let current = *self.indents.last().expect("indent stack is never empty");
        if width > current {
            self.indents.push(width);
            self.push(TokenKind::Indent, start, cursor);
        } else if width < current {
            while self.indents.last().is_some_and(|indent| *indent > width) {
                self.indents.pop();
                self.push(TokenKind::Dedent, start, cursor);
            }
            if self.indents.last().is_none_or(|indent| *indent != width) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M011003",
                        self.span(start, cursor),
                        "indentation does not match an earlier block",
                    )
                    .with_primary_label(format!("found {width} spaces"))
                    .with_note("align this line with a previous indentation level"),
                );
                self.indents.push(width);
                self.push(TokenKind::Indent, start, cursor);
            }
        }
    }

    fn lex_identifier(&mut self, start: usize, end: usize) -> usize {
        let mut cursor = start + 1;
        while cursor < end && is_identifier_continue(self.bytes[cursor]) {
            cursor += 1;
        }
        let text = &self.source.text()[start..cursor];
        let kind = keyword(text).unwrap_or_else(|| TokenKind::Identifier(text.to_owned()));
        self.push(kind, start, cursor);
        cursor
    }

    fn lex_number(&mut self, start: usize, end: usize) -> usize {
        let mut cursor = start;
        while cursor < end && self.bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        let mut decimal = false;
        if cursor + 1 < end && self.bytes[cursor] == b'.' && self.bytes[cursor + 1].is_ascii_digit()
        {
            decimal = true;
            cursor += 1;
            while cursor < end && self.bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
        }
        let number_end = cursor;
        let suffix = self.bytes.get(cursor).copied();
        if matches!(suffix, Some(b'f' | b's')) {
            cursor += 1;
        }
        let raw = &self.source.text()[start..number_end];
        let kind = match suffix {
            Some(b'f') if !decimal => raw.parse::<u32>().map(TokenKind::Frames).map_err(|_| ()),
            Some(b'f') => {
                self.invalid_number(start, cursor, "frame durations require an integer");
                return cursor;
            }
            Some(b's') => raw.parse::<f64>().map(TokenKind::Seconds).map_err(|_| ()),
            _ if decimal => raw.parse::<f64>().map(TokenKind::Num).map_err(|_| ()),
            _ => raw.parse::<i64>().map(TokenKind::Int).map_err(|_| ()),
        };
        match kind {
            Ok(kind) => self.push(kind, start, cursor),
            Err(()) => self.invalid_number(start, cursor, "numeric literal is out of range"),
        }
        cursor
    }

    fn invalid_number(&mut self, start: usize, end: usize, detail: &'static str) {
        self.diagnostics.push(
            Diagnostic::error("M011007", self.span(start, end), "invalid numeric literal")
                .with_primary_label(detail),
        );
    }

    fn lex_text(&mut self, start: usize, end: usize) -> usize {
        let mut cursor = start + 1;
        let mut decoded = String::new();
        while cursor < end {
            match self.bytes[cursor] {
                b'"' => {
                    cursor += 1;
                    self.push(TokenKind::Text(decoded), start, cursor);
                    return cursor;
                }
                b'\\' if cursor + 1 < end => {
                    let escaped = self.bytes[cursor + 1];
                    match escaped {
                        b'n' => decoded.push('\n'),
                        b't' => decoded.push('\t'),
                        b'"' => decoded.push('"'),
                        b'\\' => decoded.push('\\'),
                        _ => self.diagnostics.push(
                            Diagnostic::error(
                                "M011006",
                                self.span(cursor, cursor + 2),
                                "unknown text escape",
                            )
                            .with_primary_label(r#"valid escapes are \n, \t, \" and \\"#),
                        ),
                    }
                    if !matches!(escaped, b'n' | b't' | b'"' | b'\\') {
                        decoded.push(char::from(escaped));
                    }
                    cursor += 2;
                }
                byte if byte.is_ascii() => {
                    decoded.push(char::from(byte));
                    cursor += 1;
                }
                _ => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M011001",
                            self.span(cursor, cursor + 1),
                            "MODL/1 source must contain ASCII characters only",
                        )
                        .with_primary_label("non-ASCII byte starts here"),
                    );
                    cursor += 1;
                }
            }
        }
        self.diagnostics.push(
            Diagnostic::error(
                "M011005",
                self.span(start, end),
                "unterminated text literal",
            )
            .with_primary_label("add a closing quote before the end of the line"),
        );
        end
    }

    fn lex_asset(&mut self, start: usize, end: usize) -> usize {
        let mut cursor = start + 1;
        if cursor >= end || !is_identifier_start(self.bytes[cursor]) {
            self.diagnostics.push(
                Diagnostic::error(
                    "M011008",
                    self.span(start, cursor),
                    "asset marker must be followed by a name",
                )
                .with_primary_label("expected an asset name such as `#hero`"),
            );
            return cursor;
        }
        cursor += 1;
        while cursor < end && is_identifier_continue(self.bytes[cursor]) {
            cursor += 1;
        }
        self.push(
            TokenKind::Asset(self.source.text()[start + 1..cursor].to_owned()),
            start,
            cursor,
        );
        cursor
    }

    fn lex_comment(&mut self, start: usize, end: usize) {
        let text = self.source.text()[start + 2..end].trim_end().to_owned();
        self.push(TokenKind::Comment(text), start, end);
    }

    fn lex_symbol(&mut self, start: usize, end: usize) -> usize {
        if self.bytes.get(start..start + 3) == Some(b">>>") {
            self.push(TokenKind::ShiftRightUnsigned, start, start + 3);
            return start + 3;
        }
        let two = self.bytes.get(start..start + 2);
        let (kind, width) = match two {
            Some(b"->") => (Some(TokenKind::Arrow), 2),
            Some(b"<<") => (Some(TokenKind::ShiftLeft), 2),
            Some(b">>") => (Some(TokenKind::ShiftRight), 2),
            Some(b"..") => (Some(TokenKind::Range), 2),
            Some(b"==") => (Some(TokenKind::EqualEqual), 2),
            Some(b"!=") => (Some(TokenKind::BangEqual), 2),
            Some(b"<=") => (Some(TokenKind::LessEqual), 2),
            Some(b">=") => (Some(TokenKind::GreaterEqual), 2),
            Some(b"+=") => (Some(TokenKind::PlusEqual), 2),
            Some(b"-=") => (Some(TokenKind::MinusEqual), 2),
            Some(b"*=") => (Some(TokenKind::StarEqual), 2),
            Some(b"/=") => (Some(TokenKind::SlashEqual), 2),
            _ => (
                match self.bytes[start] {
                    b'(' => Some(TokenKind::LeftParen),
                    b')' => Some(TokenKind::RightParen),
                    b'[' => Some(TokenKind::LeftBracket),
                    b']' => Some(TokenKind::RightBracket),
                    b':' => Some(TokenKind::Colon),
                    b',' => Some(TokenKind::Comma),
                    b'.' => Some(TokenKind::Dot),
                    b'+' => Some(TokenKind::Plus),
                    b'-' => Some(TokenKind::Minus),
                    b'*' => Some(TokenKind::Star),
                    b'/' => Some(TokenKind::Slash),
                    b'%' => Some(TokenKind::Percent),
                    b'&' => Some(TokenKind::Ampersand),
                    b'|' => Some(TokenKind::Pipe),
                    b'^' => Some(TokenKind::Caret),
                    b'~' => Some(TokenKind::Tilde),
                    b'=' => Some(TokenKind::Equal),
                    b'<' => Some(TokenKind::Less),
                    b'>' => Some(TokenKind::Greater),
                    _ => None,
                },
                1,
            ),
        };
        let cursor = (start + width).min(end);
        if let Some(kind) = kind {
            self.push(kind, start, cursor);
        } else {
            let character = char::from(self.bytes[start]);
            self.diagnostics.push(
                Diagnostic::error(
                    "M011004",
                    self.span(start, cursor),
                    format!("`{character}` is not valid MODL syntax"),
                )
                .with_primary_label("unexpected character"),
            );
        }
        cursor
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        let span = self.span(start, end);
        self.tokens.push(Token::new(kind, span));
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(
            self.source.id(),
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        )
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn keyword(text: &str) -> Option<TokenKind> {
    Some(match text {
        "import" => TokenKind::Import,
        "as" => TokenKind::As,
        "pub" => TokenKind::Pub,
        "private" => TokenKind::Private,
        "const" => TokenKind::Const,
        "state" => TokenKind::State,
        "let" => TokenKind::Let,
        "var" => TokenKind::Var,
        "fn" => TokenKind::Fn,
        "task" => TokenKind::Task,
        "on" => TokenKind::On,
        "start" => TokenKind::Start,
        "update" => TokenKind::Update,
        "draw" => TokenKind::Draw,
        "raster" => TokenKind::Raster,
        "record" => TokenKind::Record,
        "enum" => TokenKind::Enum,
        "match" => TokenKind::Match,
        "case" => TokenKind::Case,
        "if" => TokenKind::If,
        "elif" => TokenKind::Elif,
        "else" => TokenKind::Else,
        "for" => TokenKind::For,
        "in" => TokenKind::In,
        "while" => TokenKind::While,
        "return" => TokenKind::Return,
        "break" => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "wait" => TokenKind::Wait,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "none" => TokenKind::None,
        "and" => TokenKind::And,
        "or" => TokenKind::Or,
        "not" => TokenKind::Not,
        "assert" => TokenKind::Assert,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::{FileId, SourceFile, TokenKind, lex};

    fn kinds(source: &str) -> Vec<TokenKind> {
        let output = lex(&SourceFile::new(FileId(0), "test.modl", source));
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        output.tokens.into_iter().map(|token| token.kind).collect()
    }

    #[test]
    fn emits_nested_indentation_and_duration_literals() {
        assert_eq!(
            kinds("task blink():\n  wait 2f\n  if true:\n    wait 0.5s\n  wait 1f\n"),
            vec![
                TokenKind::Task,
                TokenKind::Identifier("blink".to_owned()),
                TokenKind::LeftParen,
                TokenKind::RightParen,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Wait,
                TokenKind::Frames(2),
                TokenKind::Newline,
                TokenKind::If,
                TokenKind::True,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Wait,
                TokenKind::Seconds(0.5),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Wait,
                TokenKind::Frames(1),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn reports_non_ascii_and_tabs_with_stable_codes() {
        let output = lex(&SourceFile::new(FileId(0), "bad.modl", "\tlet café = 1\n"));
        let codes: Vec<_> = output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains(&"M011001"));
        assert!(codes.contains(&"M011002"));
    }

    #[test]
    fn asset_references_do_not_conflict_with_comments() {
        assert_eq!(
            kinds("let hero = #hero // sprite\n"),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("hero".to_owned()),
                TokenKind::Equal,
                TokenKind::Asset("hero".to_owned()),
                TokenKind::Comment(" sprite".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn accepts_crlf_without_leaking_carriage_returns() {
        assert_eq!(
            kinds("on update:\r\n  let frame = 1\r\n"),
            vec![
                TokenKind::On,
                TokenKind::Update,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Let,
                TokenKind::Identifier("frame".to_owned()),
                TokenKind::Equal,
                TokenKind::Int(1),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn reports_dedent_to_an_unknown_width() {
        let output = lex(&SourceFile::new(
            FileId(0),
            "bad.modl",
            "on update:\n    if true:\n        let value = 1\n  let other = 2\n",
        ));
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M011003")
        );
    }
}
