use modl_core::{cartridge::cartridge_file_stem, game::pack_game_workcart};
use std::collections::BTreeSet;

#[test]
fn cartridge_filename_stems_are_injective_and_keep_current_identity_unchanged() {
    let ids = [
        "con",
        "con.foo",
        "prn",
        "aux",
        "nul",
        "com1",
        "lpt1",
        "node",
        "foo.",
        "foo..bar",
        "a-b",
        "a.b",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ];
    let mut seen = BTreeSet::new();
    for id in ids {
        let stem = cartridge_file_stem(id).unwrap();
        assert_eq!(stem, format!("cart-{}", id.replace('.', "_")));
        assert!(!stem.contains('.'));
        for suffix in [
            ".m01c",
            ".m01c.png",
            "-native",
            ".app",
            ".bin",
            ".schema",
            ".saves",
        ] {
            let name = format!("{stem}{suffix}");
            assert!(!name.ends_with('.'));
            assert!(!name.contains(['/', '\\', ':']));
            assert!(name.split('.').next().unwrap().starts_with("cart-"));
            assert!(seen.insert(name.to_ascii_lowercase()));
        }
        let mut project =
            modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        id.clone_into(&mut project.manifest.id);
        let source =
            modl_core::encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
        let game = pack_game_workcart(&source).unwrap();
        assert_eq!(game.file_stem(), stem);
        assert_eq!(game.entry().manifest.id, id);
        assert_eq!(
            game.bytes(),
            modl_core::pack_workcart(&source).unwrap().bytes
        );
    }
}

#[test]
fn invalid_identity_never_becomes_a_host_path_component() {
    for id in [
        "",
        "ab",
        ".abc",
        "..",
        "../escape",
        "a/escape",
        "a\\escape",
        "foo_bar",
        "con:stream",
        "AUX",
        "a\0b",
        "école",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        assert_eq!(
            cartridge_file_stem(id).unwrap_err().code,
            "M014006",
            "{id:?}"
        );
    }
}
