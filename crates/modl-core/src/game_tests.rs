use super::*;
use crate::workcart::encode_workcart_with_bundle;

fn source(count: usize) -> String {
    let mut main = parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    main.manifest.id = "bundle-main".to_owned();
    let mut children = Vec::new();
    for index in 1..count {
        let mut child = main.clone();
        child.manifest.id = format!("bundle-level-{index}");
        child.files.insert(
            child.manifest.entry.clone(),
            format!("on draw:\n  clear({})\n", index + 1).into_bytes(),
        );
        children.push(BundleSource {
            alias: format!("level-{index}"),
            source: encode_workcart(&child.manifest, &child.files, &child.recipes, &child.tests)
                .unwrap(),
        });
    }
    encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &main.recipes,
        &main.tests,
        &children,
    )
    .unwrap()
}

fn entries(game: &PackedGame) -> BTreeMap<String, Vec<u8>> {
    decode_archive(game.bytes(), GAME_LIMITS).unwrap()
}

fn encode(entries: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    encode_archive_with_limits(entries, GAME_LIMITS).unwrap()
}

/// Simulates a malicious writer who can recompute all ordinary archive/index hashes.
fn rebind_index(entries: &mut BTreeMap<String, Vec<u8>>) {
    let mut index: BundleIndex = serde_json::from_slice(&entries[INDEX_PATH]).unwrap();
    index.source = integrity(&entries[SOURCE_PATH]).unwrap();
    let mut members = BTreeMap::new();
    for (alias, expected) in &mut index.members {
        let bytes = entries[&member_path(alias)].clone();
        *expected = integrity(&bytes).unwrap();
        members.insert(
            alias.clone(),
            PackedCartridge {
                manifest: decode_cartridge(&bytes).unwrap().manifest,
                bytes,
            },
        );
    }
    index.catalog_sha256 = admitted_catalog(members).identity().to_owned();
    entries.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
}

#[test]
fn single_games_preserve_existing_cartridge_bytes_exact_source_and_machine_contract() {
    for source in [
        include_str!("../../../templates/blank/cart.m01w"),
        include_str!("../../../cartridges/prism-corridor/cart.m01w"),
    ] {
        let old = crate::pack_workcart(source).unwrap();
        let game = pack_game_workcart(source).unwrap();
        assert_eq!(game.bytes(), old.bytes);
        assert_eq!(game.entry(), &old);
        assert_eq!(game.source(), source);
        assert!(!game.is_bundle());
        assert_eq!(decode_game(game.bytes()).unwrap(), game);
        assert_eq!(game.identity(), sha256_hex(game.bytes()));
    }
}

#[test]
fn flat_32_member_game_roundtrips_exact_enclosing_source_in_one_current_archive() {
    let source = format!("# exact enclosing comment\n{}", source(32)).replace('\n', "\r\n");
    let game = pack_game_workcart(&source).unwrap();
    assert!(game.is_bundle());
    assert_eq!(game.catalog().cartridges().len(), 32);
    assert_eq!(&game.bytes()[..8], b"MOD-01\x1a\x02");
    assert_eq!(first_archive_path(game.bytes()).unwrap(), INDEX_PATH);
    assert_eq!(game.source(), source);
    for (alias, member) in game.catalog().cartridges() {
        assert!(member.bytes.len() <= CARTRIDGE_CAPACITY_BYTES);
        let (decoded, _) = load_cartridge_program(&member.bytes).unwrap();
        assert_eq!(decoded.manifest.hardware_revision, 2);
        assert!(
            parse_workcart(&unpack_workcart(&member.bytes).unwrap())
                .unwrap()
                .bundle
                .is_empty()
        );
        assert!(valid_alias(alias));
    }
    let loaded = decode_game(game.bytes()).unwrap();
    assert_eq!(loaded, game);
    assert_eq!(
        pack_game_workcart(&source).unwrap().into_bytes(),
        game.bytes()
    );
    // Entry-only tools cannot accidentally treat the shipping package as one oversized ROM.
    assert!(decode_cartridge(game.bytes()).is_err());
    assert_ne!(game.bytes(), game.entry().bytes);
}

#[test]
fn invalid_index_unknown_unlisted_and_missing_members_are_rejected() {
    let game = pack_game_workcart(&source(2)).unwrap();
    let original = entries(&game);
    let mut extra = original.clone();
    extra.insert("unlisted.txt".to_owned(), b"not declared".to_vec());
    assert!(decode_game(&encode(&extra)).is_err());
    let mut missing = original.clone();
    missing.remove("cartridges/level-1.m01c");
    assert!(decode_game(&encode(&missing)).is_err());
    let mut noncanonical = original.clone();
    noncanonical.get_mut(INDEX_PATH).unwrap().push(b' ');
    assert_eq!(
        decode_game(&encode(&noncanonical)).unwrap_err().code,
        "M014074"
    );
    let mut unknown = original.clone();
    let mut index: serde_json::Value = serde_json::from_slice(&unknown[INDEX_PATH]).unwrap();
    index["unknown"] = true.into();
    unknown.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
    assert!(
        decode_game(&encode(&unknown))
            .unwrap_err()
            .message
            .contains("unknown field")
    );
    // Keep canonical field ordering, so these exercise semantic checks rather than
    // merely getting rejected because serde_json::Value sorts object keys.
    for field in [
        "formatRevision",
        "kind",
        "entry",
        "catalogSha256",
        "sourceSize",
        "memberHash",
    ] {
        let mut mutated = original.clone();
        let mut index: BundleIndex = serde_json::from_slice(&mutated[INDEX_PATH]).unwrap();
        match field {
            "formatRevision" => index.format_revision = 1,
            "kind" => index.kind = "historical".to_owned(),
            "entry" => index.entry = "level-1".to_owned(),
            "catalogSha256" => index.catalog_sha256 = "00".repeat(32),
            "sourceSize" => index.source.bytes += 1,
            "memberHash" => index.members.get_mut("main").unwrap().sha256.clear(),
            _ => unreachable!(),
        }
        mutated.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
        assert!(decode_game(&encode(&mutated)).is_err(), "{field}");
    }
    let mut invalid_alias = original;
    let mut index: BundleIndex = serde_json::from_slice(&invalid_alias[INDEX_PATH]).unwrap();
    let member = index.members.remove("level-1").unwrap();
    index.members.insert("../outside".to_owned(), member);
    invalid_alias.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
    assert!(decode_game(&encode(&invalid_alias)).is_err());
}

#[test]
fn recomputed_hashes_cannot_rebind_member_aliases_or_replace_authored_member_content() {
    let game = pack_game_workcart(&source(2)).unwrap();
    let mut swapped = entries(&game);
    let main = swapped.remove("cartridges/main.m01c").unwrap();
    let child = swapped
        .insert("cartridges/level-1.m01c".to_owned(), main)
        .unwrap();
    swapped.insert("cartridges/main.m01c".to_owned(), child);
    rebind_index(&mut swapped);
    assert_eq!(decode_game(&encode(&swapped)).unwrap_err().code, "M014074");
    let mut source_mismatch = entries(&game);
    let project = parse_workcart(game.source()).unwrap();
    let mut altered = project.bundle.clone();
    altered[0]
        .source
        .push_str("\n# source changed but archived member did not\n");
    let source = encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
        &altered,
    )
    .unwrap();
    source_mismatch.insert(SOURCE_PATH.to_owned(), source.into_bytes());
    rebind_index(&mut source_mismatch);
    assert_eq!(
        decode_game(&encode(&source_mismatch)).unwrap_err().code,
        "M014074"
    );
}

#[test]
fn forged_compiled_member_is_rejected_before_any_javascript_can_execute() {
    let game = pack_game_workcart(&source(2)).unwrap();
    let mut package = entries(&game);
    let mut member = decode_archive(
        &package["cartridges/level-1.m01c"],
        crate::cartridge::CARTRIDGE_ARCHIVE_LIMITS,
    )
    .unwrap();
    member
        .get_mut("build/cartridge.js")
        .unwrap()
        .extend_from_slice(b"\nglobalThis.FORGED = true;\n");
    let mut manifest: crate::PackedManifest =
        serde_json::from_slice(&member["manifest.json"]).unwrap();
    let forged = integrity(&member["build/cartridge.js"]).unwrap();
    manifest
        .files
        .insert("build/cartridge.js".to_owned(), forged.clone());
    manifest.compiled_program = Some(forged);
    member.insert(
        "manifest.json".to_owned(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let member =
        encode_archive_with_limits(&member, crate::cartridge::CARTRIDGE_ARCHIVE_LIMITS).unwrap();
    decode_cartridge(&member)
        .expect("ordinary structural/integrity validation alone is not source recompilation");
    package.insert("cartridges/level-1.m01c".to_owned(), member);
    rebind_index(&mut package);
    assert_eq!(decode_game(&encode(&package)).unwrap_err().code, "M014074");
}

#[test]
fn enclosing_source_comments_affect_game_identity_but_not_logical_member_roms() {
    let source = source(2);
    let first = pack_game_workcart(&source).unwrap();
    let changed = pack_game_workcart(&format!("# another enclosing comment\n{source}")).unwrap();
    assert_ne!(first.identity(), changed.identity());
    assert_eq!(first.catalog(), changed.catalog());
    assert_ne!(first.source(), changed.source());
}

#[test]
fn truncated_corrupt_old_and_nested_compositions_fail_without_fallback_or_recursive_decoding() {
    let game = pack_game_workcart(&source(2)).unwrap();
    for length in [0, 7, 12, 53, game.bytes().len() - 1] {
        assert!(decode_game(&game.bytes()[..length]).is_err());
    }
    let mut old = game.bytes().to_vec();
    old[7] = 1;
    assert!(decode_game(&old).is_err());
    let mut trailing = game.bytes().to_vec();
    trailing.push(0);
    assert!(decode_game(&trailing).is_err());
    let mut nested = entries(&game);
    nested.insert("cartridges/level-1.m01c".to_owned(), game.bytes().to_vec());
    let mut index: BundleIndex = serde_json::from_slice(&nested[INDEX_PATH]).unwrap();
    index
        .members
        .insert("level-1".to_owned(), integrity(game.bytes()).unwrap());
    nested.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
    assert!(decode_game(&encode(&nested)).is_err());
    let mut oversized = entries(&game);
    let large = vec![0; CARTRIDGE_CAPACITY_BYTES + 1];
    oversized.insert("cartridges/level-1.m01c".to_owned(), large.clone());
    let mut index: BundleIndex = serde_json::from_slice(&oversized[INDEX_PATH]).unwrap();
    index
        .members
        .insert("level-1".to_owned(), integrity(&large).unwrap());
    oversized.insert(INDEX_PATH.to_owned(), serde_json::to_vec(&index).unwrap());
    assert_eq!(
        decode_game(&encode(&oversized)).unwrap_err().code,
        "M014010"
    );
}

#[test]
fn a_logical_archive_cannot_conceal_dependencies_in_its_embedded_source() {
    let game = pack_game_workcart(&source(2)).unwrap();
    let mut member = decode_archive(
        &game.entry().bytes,
        crate::cartridge::CARTRIDGE_ARCHIVE_LIMITS,
    )
    .unwrap();
    member.insert(SOURCE_PATH.to_owned(), game.source().as_bytes().to_vec());
    let mut manifest: crate::PackedManifest =
        serde_json::from_slice(&member["manifest.json"]).unwrap();
    manifest.files.insert(
        SOURCE_PATH.to_owned(),
        integrity(&member[SOURCE_PATH]).unwrap(),
    );
    member.insert(
        "manifest.json".to_owned(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let forged =
        encode_archive_with_limits(&member, crate::cartridge::CARTRIDGE_ARCHIVE_LIMITS).unwrap();
    assert_eq!(decode_game(&forged).unwrap_err().code, "M014073");
    assert_eq!(load_cartridge_program(&forged).unwrap_err().code, "M014073");
}
