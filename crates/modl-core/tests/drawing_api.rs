use modl_core::{AssetCatalog, CompileMode, FileId, SourceFile, compile};

#[test]
fn transparency_and_patterns_are_typed_in_both_compiler_modes() {
    let source = SourceFile::new(
        FileId(0),
        "drawing.modl",
        "on draw:\n  palt(255, true)\n  palt_reset()\n  fill_pattern(65535, 255, false)\n  rect_fill(1, 2, 8, 8, 7)\n  fill_pattern_reset()\n",
    );
    for mode in [CompileMode::Release, CompileMode::Debug] {
        let result = compile(&source, &AssetCatalog::default(), mode);
        assert!(
            result.generated.is_some(),
            "{:?}",
            result.analysis.diagnostics
        );
    }
    for statement in [
        "palt(256, true)",
        "palt(1, 1)",
        "fill_pattern(0.5, 1, false)",
        "fill_pattern(1, true, false)",
        "fill_pattern_reset(1)",
    ] {
        let source = SourceFile::new(
            FileId(0),
            "invalid.modl",
            format!("on draw:\n  {statement}\n"),
        );
        let result = compile(&source, &AssetCatalog::default(), CompileMode::Release);
        assert!(result.generated.is_none(), "{statement}");
        assert!(!result.analysis.diagnostics.is_empty(), "{statement}");
    }
}

#[test]
fn frame_draw_state_cannot_be_changed_from_a_raster_callback() {
    for statement in [
        "palt(1, true)",
        "palt_reset()",
        "fill_pattern(1, 7, false)",
        "fill_pattern_reset()",
    ] {
        let source = SourceFile::new(
            FileId(0),
            "invalid.modl",
            format!("on raster(line: Int):\n  {statement}\n"),
        );
        let result = compile(&source, &AssetCatalog::default(), CompileMode::Release);
        assert!(result.generated.is_none(), "{statement}");
        assert!(
            result
                .analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("raster")),
            "{:?}",
            result.analysis.diagnostics
        );
    }
}
