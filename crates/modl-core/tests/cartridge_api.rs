use modl_core::{AssetCatalog, CompileMode, FileId, SourceFile, compile};

#[test]
fn local_cartridge_controls_are_typed_and_forbidden_in_raster() {
    for (source, valid) in [
        (
            "on start:\n  let parameter = cart_parameter()\n  load_cart(\"arena\", parameter)\n",
            true,
        ),
        ("on update:\n  reload_assets(\"bank\")\n", true),
        ("on start:\n  load_cart(\"arena\")\n", false),
        ("on start:\n  load_cart(\"arena\", 7)\n", false),
        ("on start:\n  load_cart(1, \"state\")\n", false),
        (
            "on start:\n  let parameter: Int = cart_parameter()\n",
            false,
        ),
        ("on start:\n  cart_parameter(true)\n", false),
        ("on start:\n  reload_assets(true)\n", false),
        ("on start:\n  reload_assets(\"bank\", \"extra\")\n", false),
        (
            "on raster(line: Int):\n  load_cart(\"arena\", \"\")\n",
            false,
        ),
        ("on raster(line: Int):\n  reload_assets(\"bank\")\n", false),
        ("on raster(line: Int):\n  cart_parameter()\n", false),
    ] {
        for mode in [CompileMode::Release, CompileMode::Debug] {
            let output = compile(
                &SourceFile::new(FileId(0), "cartridge.modl", source),
                &AssetCatalog::default(),
                mode,
            );
            assert_eq!(
                output.generated.is_some(),
                valid,
                "{source}: {:?}",
                output.analysis.diagnostics
            );
        }
    }
}
