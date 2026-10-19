use modl_core::{AssetCatalog, CompileMode, FileId, SourceFile, compile};

#[test]
fn the_current_compiler_accepts_every_master_colour_and_rejects_overflow() {
    for color in 0..256 {
        let source = SourceFile::new(
            FileId(0),
            "palette.modl",
            format!("state ink: Color = {color}\non draw:\n  clear(ink)\n  pixel(0, 0, {color})\n"),
        );
        for mode in [CompileMode::Release, CompileMode::Debug] {
            let compiled = compile(&source, &AssetCatalog::default(), mode);
            assert!(
                compiled.generated.is_some(),
                "colour {color}: {:?}",
                compiled.analysis.diagnostics
            );
        }
    }
    let source = SourceFile::new(FileId(0), "palette.modl", "on draw:\n  clear(256)\n");
    let output = compile(&source, &AssetCatalog::default(), CompileMode::Release);
    assert!(output.generated.is_none());
    assert!(
        output
            .analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "M013119" && diagnostic.message.contains("255"))
    );
}
