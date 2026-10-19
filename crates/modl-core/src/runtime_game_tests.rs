use super::*;
use crate::{
    bundle::BundleSource,
    cartridge::CARTRIDGE_CAPACITY_BYTES,
    encode_workcart,
    game::{decode_game, pack_game_workcart},
    load_cartridge_program, parse_workcart,
    workcart::encode_workcart_with_bundle,
};

fn bundled_source(count: usize) -> String {
    let mut root = parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    root.manifest.id = "runtime-main".to_owned();
    root.files.insert(
        root.manifest.entry.clone(),
        include_str!("../../../tests/conformance/game-main.modl")
            .as_bytes()
            .to_vec(),
    );
    let children = (1..count)
        .map(|index| {
            let mut child = root.clone();
            child.manifest.id = format!("runtime-child-{index}");
            child.manifest.update_rate = 30;
            child.files.insert(
                child.manifest.entry.clone(),
                include_str!("../../../tests/conformance/game-arena.modl")
                    .as_bytes()
                    .to_vec(),
            );
            BundleSource {
                alias: if index == 1 {
                    "arena".to_owned()
                } else {
                    format!("child{index}")
                },
                source: encode_workcart(&child.manifest, &child.files, &[], &[]).unwrap(),
            }
        })
        .collect::<Vec<_>>();
    encode_workcart_with_bundle(&root.manifest, &root.files, &[], &[], &children).unwrap()
}

#[test]
fn one_shared_handoff_preserves_every_logical_rom_and_both_compiler_modes() {
    let source = bundled_source(32);
    let packed = pack_game_workcart(&source).unwrap();
    let admitted = decode_game(packed.bytes()).unwrap();
    for mode in [CompileMode::Release, CompileMode::Debug] {
        let runtime = prepare_runtime_game(&admitted, mode).unwrap();
        assert_eq!(runtime.identity(), admitted.identity());
        assert_eq!(runtime.debug(), mode == CompileMode::Debug);
        assert_eq!(runtime.members().len(), 32);
        let json = serde_json::to_value(&runtime).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 4);
        assert_eq!(json["revision"], 1);
        for (alias, member) in runtime.members() {
            let original = &admitted.catalog().cartridges()[alias];
            let rom = STANDARD.decode(&member.rom).unwrap();
            assert_eq!(rom, original.bytes);
            assert!(rom.len() <= CARTRIDGE_CAPACITY_BYTES);
            assert_ne!(rom, admitted.bytes());
            assert_eq!(member.manifest.id, original.manifest.id);
            assert_eq!(
                member.manifest.update_rate,
                if alias == "main" { 60 } else { 30 }
            );
            assert_eq!(member.manifest.hardware_revision, 2);
            assert!(member.files.is_empty());
            assert!(member.symbol_names.values().any(|name| name == "visits"));
            let view = unpack_cartridge_project(&rom).unwrap();
            let compilation = compile_project(&view.manifest, &view.files, mode).unwrap();
            let generated = compilation.generated.unwrap();
            assert_eq!(member.javascript, generated.javascript);
            assert_eq!(member.source_map, generated.source_map_json);
            assert_eq!(json["members"][alias]["sourceMap"], member.source_map);
            if mode == CompileMode::Release {
                assert_eq!(
                    member.javascript.as_bytes(),
                    load_cartridge_program(&rom).unwrap().1
                );
            }
        }
        assert_eq!(
            serde_json::to_vec(&runtime).unwrap(),
            serde_json::to_vec(&prepare_runtime_game(&packed, mode).unwrap()).unwrap()
        );
    }
}

#[test]
fn transports_only_declared_asset_and_display_data_not_authoring_files() {
    let source = include_str!("../../../examples/api-tour/cart.m01w");
    let mut project = parse_workcart(source).unwrap();
    project.files.insert(
        "notes/private.txt".to_owned(),
        b"not a runtime import".to_vec(),
    );
    let source = encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
    let game = pack_game_workcart(&source).unwrap();
    let runtime = prepare_runtime_game(&game, CompileMode::Release).unwrap();
    let member = &runtime.members()["main"];
    assert_eq!(member.manifest.id, project.manifest.id);
    assert_eq!(STANDARD.decode(&member.rom).unwrap(), game.bytes());
    assert!(!member.files.contains_key("notes/private.txt"));
    assert!(!member.files.contains_key(&project.manifest.entry));
    let paths = project
        .manifest
        .assets
        .values()
        .map(|asset| asset.path.as_str())
        .chain(project.manifest.display.as_deref())
        .collect::<BTreeSet<_>>();
    assert_eq!(member.files.len(), paths.len());
    for path in paths {
        assert_eq!(
            STANDARD.decode(&member.files[path]).unwrap(),
            project.files[path]
        );
    }
}

#[test]
fn members_keep_independent_symbols_for_identical_source_paths_and_pack_identity() {
    let source = bundled_source(2);
    let game = pack_game_workcart(&source).unwrap();
    let runtime = prepare_runtime_game(&game, CompileMode::Debug).unwrap();
    let main = &runtime.members()["main"];
    let arena = &runtime.members()["arena"];
    assert_ne!(main.javascript, arena.javascript);
    assert_ne!(main.source_map, arena.source_map);
    let main_map: serde_json::Value = serde_json::from_str(&main.source_map).unwrap();
    let arena_map: serde_json::Value = serde_json::from_str(&arena.source_map).unwrap();
    assert_eq!(main_map["sources"], arena_map["sources"]);
    assert_eq!(main.manifest.id, "runtime-main");
    assert_eq!(arena.manifest.id, "runtime-child-1");
    let changed = pack_game_workcart(&format!("# whole-game identity changes\n{source}")).unwrap();
    let other = prepare_runtime_game(&changed, CompileMode::Debug).unwrap();
    assert_ne!(runtime.identity(), other.identity());
    assert_eq!(runtime.members(), other.members());
}

#[test]
fn source_transport_limit_is_explicit_and_has_an_exact_inclusive_boundary() {
    assert!(check_program_size(MAX_RUNTIME_JAVASCRIPT_BYTES).is_ok());
    assert_eq!(
        check_program_size(MAX_RUNTIME_JAVASCRIPT_BYTES + 1)
            .unwrap_err()
            .code,
        "M014072"
    );
}
