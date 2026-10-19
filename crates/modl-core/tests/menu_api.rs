use modl_core::{AssetCatalog, CompileMode, FileId, SourceFile, compile};

#[test]
fn menu_registration_and_callback_have_one_current_typed_signature() {
    for (text, valid) in [
        (
            "on start:\n  menu_item(1, \"RESET PUZZLE\")\non menu(action: Int):\n  assert action == 1, \"slot\"\n",
            true,
        ),
        ("on menu:\n  menu_item(1, \"RESET\")\n", false),
        ("on menu(action: Bool):\n  menu_item(1, \"RESET\")\n", false),
        (
            "on menu(action: Int, extra: Int):\n  menu_item(1, \"RESET\")\n",
            false,
        ),
        (
            "on menu(action: Int):\n  menu_item(1, \"RESET\")\non menu(other: Int):\n  menu_item(2, \"OTHER\")\n",
            false,
        ),
        ("on start:\n  menu_item(true, \"RESET\")\n", false),
        ("on start:\n  menu_item(1, 7)\n", false),
        ("on start:\n  menu_item(1, \"RESET\", true)\n", false),
        ("on raster(line: Int):\n  menu_item(1, \"RESET\")\n", false),
        ("on raster(line: Bool):\n  pal(0, 1)\n", false),
    ] {
        for mode in [CompileMode::Release, CompileMode::Debug] {
            let source = SourceFile::new(FileId(0), "menu.modl", text);
            let result = compile(&source, &AssetCatalog::default(), mode);
            assert_eq!(
                result.generated.is_some(),
                valid,
                "{text}: {:?}",
                result.analysis.diagnostics
            );
            if valid {
                assert!(
                    result
                        .generated
                        .unwrap()
                        .javascript
                        .contains("menu:action=>")
                );
            }
        }
    }
}
