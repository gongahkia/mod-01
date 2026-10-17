//! MODL/1 source coloring for the native editor.
use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId, Galley, Ui, text::LayoutJob};
use modl_core::{FileId, SourceFile, TokenKind, lex};

use crate::ui_theme::{AMBER, CORAL, CREAM, CYAN};

const COMMENT: Color32 = Color32::from_rgb(154, 146, 127);
const VIOLET: Color32 = Color32::from_rgb(191, 163, 218);

/// Caches lexical work while egui asks for a layout on every frame.
#[derive(Default)]
pub struct ModlHighlighter {
    source: String,
    job: LayoutJob,
}

impl ModlHighlighter {
    pub fn layout(&mut self, ui: &Ui, source: &str, wrap_width: f32) -> Arc<Galley> {
        if source != self.source {
            self.job = highlight(source);
            self.source.clear();
            self.source.push_str(source);
        }
        let mut job = self.job.clone();
        job.wrap.max_width = wrap_width;
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    }
}

fn highlight(source: &str) -> LayoutJob {
    let file = SourceFile::new(FileId(0), "editor", source);
    let lexed = lex(&file);
    let mut colored = Vec::with_capacity(lexed.tokens.len() + lexed.diagnostics.len());
    for token in lexed.tokens {
        let start = token.span.start as usize;
        let end = token.span.end as usize;
        if start < end
            && end <= source.len()
            && source.is_char_boundary(start)
            && source.is_char_boundary(end)
        {
            if let Some(color) = color_for(&token.kind) {
                colored.push((start, end, color));
            }
        }
    }
    // The compiler consumes an unterminated string without emitting a Text token.
    // Keep it colored as the user types its closing quote.
    for diagnostic in lexed.diagnostics {
        if diagnostic.code == "M011005" {
            let start = diagnostic.primary.span.start as usize;
            let end = diagnostic.primary.span.end as usize;
            if start < end
                && end <= source.len()
                && source.is_char_boundary(start)
                && source.is_char_boundary(end)
            {
                colored.push((start, end, AMBER));
            }
        }
    }
    colored.sort_unstable_by_key(|(start, end, _)| (*start, *end));

    let mut job = LayoutJob::default();
    let mut cursor = 0;
    for (start, end, color) in colored {
        if start < cursor {
            continue;
        }
        append(&mut job, &source[cursor..start], CREAM);
        append(&mut job, &source[start..end], color);
        cursor = end;
    }
    append(&mut job, &source[cursor..], CREAM);
    job
}

fn append(job: &mut LayoutJob, text: &str, color: Color32) {
    if !text.is_empty() {
        job.append(
            text,
            0.0,
            egui::TextFormat::simple(FontId::new(16.0, FontFamily::Monospace), color),
        );
    }
}

fn color_for(kind: &TokenKind) -> Option<Color32> {
    match kind {
        TokenKind::Comment(_) => Some(COMMENT),
        TokenKind::Text(_) => Some(AMBER),
        TokenKind::Asset(_) => Some(VIOLET),
        TokenKind::Int(_) | TokenKind::Num(_) | TokenKind::Frames(_) | TokenKind::Seconds(_) => {
            Some(CYAN)
        }
        TokenKind::Identifier(name)
            if matches!(
                name.as_str(),
                "Int"
                    | "Num"
                    | "Bool"
                    | "Text"
                    | "Color"
                    | "Vec2"
                    | "Rect"
                    | "Unit"
                    | "Option"
                    | "List"
                    | "Controller"
                    | "Button"
                    | "Sprite"
                    | "Animation"
                    | "TileSet"
                    | "Map"
                    | "Font"
                    | "Sound"
                    | "Music"
            ) =>
        {
            Some(VIOLET)
        }
        TokenKind::Import
        | TokenKind::As
        | TokenKind::Pub
        | TokenKind::Private
        | TokenKind::Const
        | TokenKind::State
        | TokenKind::Let
        | TokenKind::Var
        | TokenKind::Fn
        | TokenKind::Task
        | TokenKind::On
        | TokenKind::Start
        | TokenKind::Update
        | TokenKind::Draw
        | TokenKind::Raster
        | TokenKind::Record
        | TokenKind::Enum
        | TokenKind::Match
        | TokenKind::Case
        | TokenKind::If
        | TokenKind::Elif
        | TokenKind::Else
        | TokenKind::For
        | TokenKind::In
        | TokenKind::While
        | TokenKind::Return
        | TokenKind::Break
        | TokenKind::Continue
        | TokenKind::Wait
        | TokenKind::True
        | TokenKind::False
        | TokenKind::None
        | TokenKind::And
        | TokenKind::Or
        | TokenKind::Not
        | TokenKind::Assert => Some(CORAL),
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => None,
        _ => Some(CREAM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_at(job: &LayoutJob, needle: &str) -> Color32 {
        let offset = job.text.find(needle).unwrap();
        job.sections
            .iter()
            .find(|section| {
                section.byte_range.start.0 <= offset && offset < section.byte_range.end.0
            })
            .unwrap()
            .format
            .color
    }

    #[test]
    fn highlights_language_tokens_without_changing_source() {
        let source = "// note\nstate score: Int = 12f\nlet title = \"HI\"\nsprite(#hero, 1, 2)\n";
        let job = highlight(source);
        assert_eq!(job.text, source);
        assert_eq!(color_at(&job, "// note"), COMMENT);
        assert_eq!(color_at(&job, "state"), CORAL);
        assert_eq!(color_at(&job, "Int"), VIOLET);
        assert_eq!(color_at(&job, "12f"), CYAN);
        assert_eq!(color_at(&job, "\"HI\""), AMBER);
        assert_eq!(color_at(&job, "#hero"), VIOLET);
        assert_eq!(color_at(&job, "sprite"), CREAM);
    }

    #[test]
    fn unfinished_and_non_ascii_input_keeps_valid_text_ranges() {
        let source = "let caption = \"unfinished\n// café\n";
        let job = highlight(source);
        assert_eq!(job.text, source);
        assert_eq!(color_at(&job, "\"unfinished"), AMBER);
        assert_eq!(color_at(&job, "// café"), COMMENT);
    }
}
