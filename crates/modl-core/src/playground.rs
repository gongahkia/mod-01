//! Source wrapping for the native playground. Evaluation always uses the ordinary compiler/core.
use crate::CartridgeError;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub const PLAYGROUND_MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const PLAYGROUND_RESULT_NAME: &str = "scratch_result";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaygroundMode {
    Expression,
    Statements,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlaygroundSource {
    pub source: String,
    pub result_name: Option<&'static str>,
}

/// Builds inspectable MODL, not JavaScript or an alternative expression interpreter.
///
/// # Errors
/// Rejects empty/oversized input and multi-line expression entries. Syntax/types are checked by
/// the ordinary project compiler after this source is inserted into an isolated temporary workcart.
pub fn playground_source(
    mode: PlaygroundMode,
    input: &str,
    definitions: &str,
) -> Result<PlaygroundSource, CartridgeError> {
    let error = |message: &str| CartridgeError {
        code: "M014031",
        message: message.to_owned(),
    };
    if input.len().saturating_add(definitions.len()) > PLAYGROUND_MAX_SOURCE_BYTES {
        return Err(error("scratchpad source exceeds 64 KiB"));
    }
    let input = input.trim_matches([' ', '\t', '\n', '\r']);
    if input.is_empty() {
        return Err(error("scratchpad input is empty"));
    }
    if mode == PlaygroundMode::Expression && input.contains(['\n', '\r']) {
        return Err(error("use statements mode for multi-line MODL"));
    }
    let mut source = definitions.to_owned();
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    source.push_str("on start:\n");
    let result_name = match mode {
        PlaygroundMode::Expression => {
            writeln!(source, "  let {PLAYGROUND_RESULT_NAME} = ({input})")
                .expect("String formatting is infallible");
            Some(PLAYGROUND_RESULT_NAME)
        }
        PlaygroundMode::Statements => {
            for line in input.lines() {
                source.push_str("  ");
                source.push_str(line);
                source.push('\n');
            }
            None
        }
    };
    // Keep the outer scope live long enough for the real debugger to observe its final values.
    source.push_str("  assert true\n");
    if source.len() > PLAYGROUND_MAX_SOURCE_BYTES {
        return Err(error("wrapped scratchpad source exceeds 64 KiB"));
    }
    Ok(PlaygroundSource {
        source,
        result_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetCatalog, CompileMode, FileId, SourceFile, Type, compile};
    #[test]
    fn expression_results_are_typed_by_the_real_compiler_in_both_modes() {
        for (input, ty) in [
            ("2 + 3", Type::Int),
            ("sqrt(81.0)", Type::Num),
            ("format_int(255, 16, 4)", Type::Text),
            ("is_some(parse_int(\"7\", 10))", Type::Bool),
        ] {
            let wrapped = playground_source(PlaygroundMode::Expression, input, "").unwrap();
            assert_eq!(wrapped.result_name, Some(PLAYGROUND_RESULT_NAME));
            for mode in [CompileMode::Release, CompileMode::Debug] {
                let compiled = compile(
                    &SourceFile::new(FileId(0), "scratch.modl", &wrapped.source),
                    &AssetCatalog::default(),
                    mode,
                );
                assert!(
                    compiled.analysis.diagnostics.is_empty(),
                    "{:?}",
                    compiled.analysis.diagnostics
                );
                assert!(compiled.generated.is_some());
                assert_eq!(
                    compiled
                        .analysis
                        .symbols
                        .iter()
                        .find(|symbol| symbol.name == PLAYGROUND_RESULT_NAME)
                        .unwrap()
                        .r#type,
                    ty
                );
            }
        }
    }
    #[test]
    fn statements_and_definitions_preserve_real_modl_and_reject_invalid_source() {
        let wrapped = playground_source(
            PlaygroundMode::Statements,
            "clear(25)\nif true:\n  pixel(twice(3), 2, 255)",
            "fn twice(value: Int) -> Int:\n  return value * 2\n",
        )
        .unwrap();
        assert!(wrapped.result_name.is_none());
        let compiled = compile(
            &SourceFile::new(FileId(0), "scratch.modl", wrapped.source),
            &AssetCatalog::default(),
            CompileMode::Debug,
        );
        assert!(
            compiled.analysis.diagnostics.is_empty(),
            "{:?}",
            compiled.analysis.diagnostics
        );
        let invalid = playground_source(PlaygroundMode::Expression, "sqrt(true)", "").unwrap();
        assert!(
            compile(
                &SourceFile::new(FileId(0), "scratch.modl", invalid.source),
                &AssetCatalog::default(),
                CompileMode::Debug
            )
            .generated
            .is_none()
        );
        assert!(playground_source(PlaygroundMode::Expression, "1\n2", "").is_err());
        assert!(playground_source(PlaygroundMode::Expression, " ", "").is_err());
        assert!(
            playground_source(
                PlaygroundMode::Statements,
                &"x".repeat(PLAYGROUND_MAX_SOURCE_BYTES + 1),
                ""
            )
            .is_err()
        );
    }
}
