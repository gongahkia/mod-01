//! Shared compiler, analysis, and cartridge-format implementation for MODL/1.

pub mod assets;
pub mod ast;
pub mod cartridge;
pub mod cartridge_png;
pub mod codegen;
pub mod diagnostic;
pub mod formatter;
pub mod generator;
pub mod hardware;
pub mod ir;
pub mod lexer;
pub mod native_export;
pub mod parser;
pub mod playground;
pub mod sema;
pub mod span;
pub mod token;
pub mod tooling;
pub mod types;
pub mod workcart;

pub use cartridge::{
    CartridgeError, DecodedCartridge, PackedCartridge, PackedManifest, ProjectAsset,
    ProjectManifest, UnpackedProject, analyze_project_model, compile_project, decode_cartridge,
    load_cartridge_program, pack_project, pack_workcart, parse_project_manifest,
    unpack_cartridge_project, unpack_workcart,
};
pub use cartridge_png::{
    CartridgePng, CartridgePngMetadata, decode_cartridge_png, encode_cartridge_png,
};
pub use codegen::{CompilationOutput, CompileMode, GeneratedProgram, compile};
pub use diagnostic::{Diagnostic, Severity};
pub use formatter::{FormatError, format_source};
pub use generator::materialize_workcart_recipe;
pub use lexer::lex;
pub use native_export::{NativePlatform, NativeToolchain, export_native_game};
pub use parser::{ParseOutput, parse};
pub use playground::{PlaygroundMode, PlaygroundSource, playground_source};
pub use sema::{AnalysisOutput, BuiltinDefinition, analyze_module, builtin_definitions};
pub use span::{FileId, LineColumn, SourceFile, Span, Spanned};
pub use token::{Token, TokenKind};
pub use types::{AssetCatalog, AssetKind, Type};
pub use workcart::{
    WORKCART_FORMAT_REVISION, WorkcartProject, WorkcartRecipe, WorkcartTest, WorkcartTestKind,
    delete_workcart_asset, encode_workcart, parse_workcart, rewrite_workcart,
    workcart_project_view,
};

/// The source-language revision understood by this compiler.
pub const LANGUAGE_REVISION: &str = "MODL/1";

/// The deterministic packed-cartridge format revision.
pub const CARTRIDGE_FORMAT_REVISION: u16 = 2;

/// Default deterministic seed used by native Studio/games and CLI headless execution.
pub const DEFAULT_SEED: u32 = 604_772_761;

/// Identifies the compiler in generated artifacts and diagnostics.
#[must_use]
pub const fn compiler_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::{CARTRIDGE_FORMAT_REVISION, LANGUAGE_REVISION, compiler_version};

    #[test]
    fn revisions_are_explicit() {
        assert_eq!(LANGUAGE_REVISION, "MODL/1");
        assert_eq!(CARTRIDGE_FORMAT_REVISION, 2);
        assert!(!compiler_version().is_empty());
    }
}
