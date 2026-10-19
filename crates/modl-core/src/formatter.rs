use std::fmt;

use crate::{
    diagnostic::Diagnostic,
    parser::parse,
    span::SourceFile,
    token::{Token, TokenKind},
};

/// Formatting failed because the input has lexical or syntactic diagnostics.
#[derive(Clone, Debug)]
pub struct FormatError {
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot format MODL source with {} diagnostic(s)",
            self.diagnostics.len()
        )
    }
}

impl std::error::Error for FormatError {}

/// Formats one valid MODL/1 module with two-space indentation and canonical token spacing.
///
/// # Errors
///
/// Returns every lexical and parsing diagnostic when the source is not valid enough to format
/// without changing its meaning.
pub fn format_source(source: &SourceFile) -> Result<String, FormatError> {
    let output = parse(source);
    if !output.diagnostics.is_empty() {
        return Err(FormatError {
            diagnostics: output.diagnostics,
        });
    }
    Ok(format_tokens(&output.tokens))
}

fn format_tokens(tokens: &[Token]) -> String {
    let mut output = String::new();
    let mut indentation = 0_usize;
    let mut line_start = true;
    let mut previous: Option<&TokenKind> = None;
    let mut previous_was_unary = false;

    for token in tokens {
        match &token.kind {
            TokenKind::Indent => indentation += 1,
            TokenKind::Dedent => indentation = indentation.saturating_sub(1),
            TokenKind::Newline => {
                while output.ends_with(' ') {
                    output.pop();
                }
                output.push('\n');
                line_start = true;
                previous = None;
                previous_was_unary = false;
            }
            TokenKind::Eof => {}
            TokenKind::Comment(text) => {
                begin_token(&mut output, indentation, &mut line_start);
                if previous.is_some() {
                    output.push(' ');
                }
                output.push_str("//");
                output.push_str(text);
                previous = Some(&token.kind);
                previous_was_unary = false;
            }
            kind => {
                begin_token(&mut output, indentation, &mut line_start);
                if !previous_was_unary
                    && previous.is_some_and(|previous| needs_space(previous, kind))
                {
                    output.push(' ');
                }
                output.push_str(&render(kind));
                previous_was_unary = is_unary_prefix(kind, previous);
                previous = Some(kind);
            }
        }
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn is_unary_prefix(current: &TokenKind, previous: Option<&TokenKind>) -> bool {
    matches!(
        current,
        TokenKind::Plus | TokenKind::Minus | TokenKind::Tilde
    ) && previous.is_none_or(|previous| {
        matches!(
            previous,
            TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::Equal
                | TokenKind::EqualEqual
                | TokenKind::BangEqual
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::Ampersand
                | TokenKind::Pipe
                | TokenKind::Caret
                | TokenKind::ShiftLeft
                | TokenKind::ShiftRight
                | TokenKind::ShiftRightUnsigned
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Return
        )
    })
}

fn begin_token(output: &mut String, indentation: usize, line_start: &mut bool) {
    if *line_start {
        for _ in 0..indentation {
            output.push_str("  ");
        }
        *line_start = false;
    }
}

fn needs_space(previous: &TokenKind, current: &TokenKind) -> bool {
    if matches!(
        current,
        TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::Dot
            | TokenKind::Range
    ) || matches!(
        previous,
        TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::Dot | TokenKind::Range
    ) {
        return false;
    }
    if matches!(current, TokenKind::LeftParen | TokenKind::LeftBracket)
        && matches!(
            previous,
            TokenKind::Identifier(_)
                | TokenKind::Raster
                | TokenKind::RightParen
                | TokenKind::RightBracket
        )
    {
        return false;
    }
    true
}

#[allow(clippy::too_many_lines)]
fn render(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Identifier(value) => value.clone(),
        TokenKind::Asset(value) => format!("#{value}"),
        TokenKind::Int(value) => value.to_string(),
        TokenKind::Num(value) => render_decimal(*value),
        TokenKind::Text(value) => render_text(value),
        TokenKind::Frames(value) => format!("{value}f"),
        TokenKind::Seconds(value) => format!("{}s", render_decimal(*value)),
        TokenKind::Comment(value) => format!("//{value}"),
        TokenKind::Import => "import".to_owned(),
        TokenKind::As => "as".to_owned(),
        TokenKind::Pub => "pub".to_owned(),
        TokenKind::Private => "private".to_owned(),
        TokenKind::Const => "const".to_owned(),
        TokenKind::State => "state".to_owned(),
        TokenKind::Let => "let".to_owned(),
        TokenKind::Var => "var".to_owned(),
        TokenKind::Fn => "fn".to_owned(),
        TokenKind::Task => "task".to_owned(),
        TokenKind::On => "on".to_owned(),
        TokenKind::Start => "start".to_owned(),
        TokenKind::Update => "update".to_owned(),
        TokenKind::Draw => "draw".to_owned(),
        TokenKind::Raster => "raster".to_owned(),
        TokenKind::Record => "record".to_owned(),
        TokenKind::Enum => "enum".to_owned(),
        TokenKind::Match => "match".to_owned(),
        TokenKind::Case => "case".to_owned(),
        TokenKind::If => "if".to_owned(),
        TokenKind::Elif => "elif".to_owned(),
        TokenKind::Else => "else".to_owned(),
        TokenKind::For => "for".to_owned(),
        TokenKind::In => "in".to_owned(),
        TokenKind::While => "while".to_owned(),
        TokenKind::Return => "return".to_owned(),
        TokenKind::Break => "break".to_owned(),
        TokenKind::Continue => "continue".to_owned(),
        TokenKind::Wait => "wait".to_owned(),
        TokenKind::True => "true".to_owned(),
        TokenKind::False => "false".to_owned(),
        TokenKind::None => "none".to_owned(),
        TokenKind::And => "and".to_owned(),
        TokenKind::Or => "or".to_owned(),
        TokenKind::Not => "not".to_owned(),
        TokenKind::Assert => "assert".to_owned(),
        TokenKind::LeftParen => "(".to_owned(),
        TokenKind::RightParen => ")".to_owned(),
        TokenKind::LeftBracket => "[".to_owned(),
        TokenKind::RightBracket => "]".to_owned(),
        TokenKind::Colon => ":".to_owned(),
        TokenKind::Comma => ",".to_owned(),
        TokenKind::Dot => ".".to_owned(),
        TokenKind::Arrow => "->".to_owned(),
        TokenKind::Range => "..".to_owned(),
        TokenKind::Plus => "+".to_owned(),
        TokenKind::Minus => "-".to_owned(),
        TokenKind::Star => "*".to_owned(),
        TokenKind::Slash => "/".to_owned(),
        TokenKind::Percent => "%".to_owned(),
        TokenKind::Ampersand => "&".to_owned(),
        TokenKind::Pipe => "|".to_owned(),
        TokenKind::Caret => "^".to_owned(),
        TokenKind::Tilde => "~".to_owned(),
        TokenKind::ShiftLeft => "<<".to_owned(),
        TokenKind::ShiftRight => ">>".to_owned(),
        TokenKind::ShiftRightUnsigned => ">>>".to_owned(),
        TokenKind::Equal => "=".to_owned(),
        TokenKind::EqualEqual => "==".to_owned(),
        TokenKind::BangEqual => "!=".to_owned(),
        TokenKind::Less => "<".to_owned(),
        TokenKind::LessEqual => "<=".to_owned(),
        TokenKind::Greater => ">".to_owned(),
        TokenKind::GreaterEqual => ">=".to_owned(),
        TokenKind::PlusEqual => "+=".to_owned(),
        TokenKind::MinusEqual => "-=".to_owned(),
        TokenKind::StarEqual => "*=".to_owned(),
        TokenKind::SlashEqual => "/=".to_owned(),
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {
            String::new()
        }
    }
}

fn render_decimal(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn render_text(value: &str) -> String {
    let mut rendered = String::from("\"");
    for character in value.chars() {
        match character {
            '\n' => rendered.push_str("\\n"),
            '\t' => rendered.push_str("\\t"),
            '"' => rendered.push_str("\\\""),
            '\\' => rendered.push_str("\\\\"),
            other => rendered.push(other),
        }
    }
    rendered.push('"');
    rendered
}

#[cfg(test)]
mod tests {
    #[test]
    fn bitwise_tokens_preserve_longest_match_and_unary_spacing() {
        let source = crate::SourceFile::new(
            crate::FileId(0),
            "bits.modl",
            "on start:\n  let value=~1&-2|3^4<<2>>1>>>0\n",
        );
        let formatted = super::format_source(&source).expect("bit expression formats");
        assert!(
            formatted.contains("~1 & -2 | 3 ^ 4 << 2 >> 1 >>> 0"),
            "{formatted}"
        );
        let again = crate::SourceFile::new(crate::FileId(0), "bits.modl", &formatted);
        assert_eq!(super::format_source(&again).expect("idempotent"), formatted);
    }

    use crate::{FileId, SourceFile, format_source};

    #[test]
    fn formats_indentation_spacing_and_comments_idempotently() {
        let source = SourceFile::new(
            FileId(0),
            "messy.modl",
            "state  score:Int=0 // kept\non update:\n        if true:\n          score+=1\n",
        );
        let formatted = format_source(&source).expect("source formats");
        assert_eq!(
            formatted,
            "state score: Int = 0 // kept\non update:\n  if true:\n    score += 1\n"
        );
        let second = SourceFile::new(FileId(0), "messy.modl", formatted.clone());
        assert_eq!(format_source(&second).expect("formats twice"), formatted);
    }

    #[test]
    fn refuses_to_rewrite_invalid_source() {
        let source = SourceFile::new(FileId(0), "bad.modl", "state score = 0\n");
        let error = format_source(&source).expect_err("missing type is invalid");
        assert!(error.diagnostics.iter().any(|item| item.code == "M012002"));
    }
}
