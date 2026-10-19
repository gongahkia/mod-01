use modl_core::{
    AssetKind, ProjectAsset, ProjectManifest,
    assets::{AssetReport, analyze_project_assets},
    parse_workcart,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn project() -> (ProjectManifest, BTreeMap<String, Vec<u8>>) {
    let project = parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    (project.manifest, project.files)
}

fn add(
    manifest: &mut ProjectManifest,
    files: &mut BTreeMap<String, Vec<u8>>,
    name: &str,
    kind: AssetKind,
    value: &Value,
) {
    let path = format!("assets/{name}.json");
    manifest.assets.insert(
        name.to_owned(),
        ProjectAsset {
            kind,
            path: path.clone(),
        },
    );
    files.insert(path, serde_json::to_vec(value).unwrap());
}

fn sound() -> Value {
    json!({"revision":1,"kind":"sound","waveform":"triangle","note":60,"durationFrames":4,"volume":0.5,"pan":0,"envelope":{"attackFrames":0,"decayFrames":0,"sustainLevel":1,"releaseFrames":1},"pitch":{"slideSemitonesPerFrame":0,"vibratoDepthSemitones":0,"vibratoPeriodFrames":0}})
}

fn single(kind: AssetKind, value: &Value) -> AssetReport {
    let (mut manifest, mut files) = project();
    add(&mut manifest, &mut files, "subject", kind, value);
    analyze_project_assets(&manifest, &files)
}

#[test]
fn every_maintained_game_example_and_template_has_valid_current_assets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut checked = 0;
    for directory in ["cartridges", "examples", "templates"] {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path().join("cart.m01w");
            if !path.is_file() {
                continue;
            }
            let project = parse_workcart(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let mut members = vec![("main".to_owned(), project.manifest, project.files)];
            for child in project.bundle {
                let parsed = parse_workcart(&child.source).unwrap();
                members.push((child.alias, parsed.manifest, parsed.files));
            }
            for (alias, manifest, files) in members {
                let report = analyze_project_assets(&manifest, &files);
                assert!(
                    report.is_valid(),
                    "{}/{alias}: {:?}",
                    path.display(),
                    report.diagnostics
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 15,
        "expected maintained project coverage, found {checked}"
    );
}

#[test]
fn byte_and_descriptor_costs_match_the_actual_current_visual_storage_layout() {
    let (mut manifest, mut files) = project();
    let sprite = json!({"revision":2,"kind":"sprite","width":2.0,"height":1,"frames":[[0,255.0],[254,1]],"memo":"retained"});
    add(
        &mut manifest,
        &mut files,
        "sprite",
        AssetKind::Sprite,
        &sprite,
    );
    add(
        &mut manifest,
        &mut files,
        "animation",
        AssetKind::Animation,
        &sprite,
    );
    add(
        &mut manifest,
        &mut files,
        "tiles",
        AssetKind::TileSet,
        &json!({"revision":2,"kind":"tile_set","tiles":[vec![1;64],vec![255;64]],"flags":[0,255]}),
    );
    add(
        &mut manifest,
        &mut files,
        "map",
        AssetKind::Map,
        &json!({"revision":1,"kind":"map","layers":[{"width":2,"height":1,"tileSet":"tiles","cells":[0,1]}]}),
    );
    add(
        &mut manifest,
        &mut files,
        "font",
        AssetKind::Font,
        &json!({"revision":1,"kind":"font","glyphWidth":2,"glyphHeight":2,"baseline":1,"advanceX":2,"advanceY":2,"missingGlyph":63,"glyphs":[{"code":63,"pixels":[0,1,1,0]}]}),
    );
    add(
        &mut manifest,
        &mut files,
        "sound",
        AssetKind::Sound,
        &sound(),
    );
    add(
        &mut manifest,
        &mut files,
        "music",
        AssetKind::Music,
        &json!({"revision":1,"kind":"music","framesPerRow":240,"loop":false,"order":["A"],"patterns":{"A":{"rows":[[{"note":127,"sound":"sound","volume":0},null,null,null,null,null,null,null]]}}}),
    );
    manifest.display = Some("assets/display.json".to_owned());
    files.insert("assets/display.json".to_owned(), serde_json::to_vec(&json!({"revision":2,"kind":"display","remap":(0..32).collect::<Vec<_>>(),"raster":[{"line":143,"scrollX":-32768,"scrollY":32767,"remap":vec![255;32]}]})).unwrap());
    let original = files.clone();
    let report = analyze_project_assets(&manifest, &files);
    assert!(report.is_valid(), "{:?}", report.diagnostics);
    assert_eq!(report.usage.visual_bytes, 224);
    assert_eq!(report.usage.visual_allocations, 10);
    assert_eq!(report.usage.visual_assets, 5);
    assert_eq!(report.usage.audio_assets, 2);
    assert_eq!(files, original);
}

#[test]
fn schema_errors_have_stable_codes_asset_paths_and_precise_fields() {
    let sprite = json!({"revision":2,"kind":"sprite","width":1,"height":1,"frames":[[255]]});
    for (pointer, invalid) in [
        ("/revision", json!(1)),
        ("/width", json!(129)),
        ("/height", json!(0)),
        ("/frames/0/0", json!(256)),
        ("/frames", json!([])),
    ] {
        let mut value = sprite.clone();
        *value.pointer_mut(pointer).unwrap() = invalid;
        let report = single(AssetKind::Sprite, &value);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.code, "M014060");
        assert_eq!(diagnostic.asset, "subject");
        assert_eq!(diagnostic.path, "assets/subject.json");
        assert!(!diagnostic.field.is_empty());
        assert!(report.require_valid().is_err());
    }
    for (pointer, invalid) in [
        ("/durationFrames", json!(0)),
        ("/note", json!(127.5)),
        ("/volume", json!(2)),
        ("/pan", json!(-2)),
        ("/envelope/releaseFrames", json!(3601)),
        ("/pitch/slideSemitonesPerFrame", json!(1e200)),
    ] {
        let mut value = sound();
        *value.pointer_mut(pointer).unwrap() = invalid;
        assert!(!single(AssetKind::Sound, &value).is_valid(), "{pointer}");
    }
    for (waveform, extra, accepted) in [
        ("pulse", json!({"duty":0.5}), true),
        ("pulse", json!({"duty":1}), false),
        ("saw", json!({}), true),
        ("noise", json!({}), true),
        ("wavetable", json!({"wavetable":[-1,0,1,0]}), true),
        ("wavetable", json!({"wavetable":[-1,2,1,0]}), false),
    ] {
        let mut value = sound();
        value["waveform"] = json!(waveform);
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            single(AssetKind::Sound, &value).is_valid(),
            accepted,
            "{waveform}: {value}"
        );
    }
}

#[test]
fn map_and_music_references_are_checked_even_in_unused_layers_and_patterns() {
    let (mut manifest, mut files) = project();
    add(
        &mut manifest,
        &mut files,
        "tiles",
        AssetKind::TileSet,
        &json!({"revision":2,"kind":"tile_set","tiles":[vec![0;64]],"flags":[0]}),
    );
    add(
        &mut manifest,
        &mut files,
        "map",
        AssetKind::Map,
        &json!({"revision":1,"kind":"map","layers":[{"width":1,"height":1,"tileSet":"tiles","cells":[1]},{"width":1,"height":1,"tileSet":"missing","cells":[0]}]}),
    );
    add(
        &mut manifest,
        &mut files,
        "music",
        AssetKind::Music,
        &json!({"revision":1,"kind":"music","framesPerRow":1,"loop":false,"order":["silent"],"patterns":{"silent":{"rows":[vec![Value::Null;8]]},"unused":{"rows":[[{"note":0,"sound":"tiles"},null,null,null,null,null,null,null]]}}}),
    );
    let report = analyze_project_assets(&manifest, &files);
    assert_eq!(report.diagnostics.len(), 3, "{:?}", report.diagnostics);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "M014061")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.field == "$.layers[0].cells[0]")
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("'tiles' must name a declared sound")
    }));
}

#[test]
fn capacity_checks_distinguish_bytes_from_allocation_slots_at_exact_boundaries() {
    let mut animation = json!({"revision":2,"kind":"sprite","width":128,"height":128,"frames":vec![vec![0;128*128];64]});
    let report = single(AssetKind::Animation, &animation);
    assert!(report.is_valid());
    assert_eq!(report.usage.visual_bytes, 1024 * 1024);
    animation["frames"]
        .as_array_mut()
        .unwrap()
        .push(json!(vec![0; 128 * 128]));
    let report = single(AssetKind::Animation, &animation);
    assert_eq!(report.diagnostics[0].code, "M014062");
    assert!(report.diagnostics[0].message.ends_with("over by 16384"));
    let (mut manifest, mut files) = project();
    let tiles =
        json!({"revision":2,"kind":"tile_set","tiles":vec![vec![0;64];4095],"flags":vec![0;4095]});
    add(
        &mut manifest,
        &mut files,
        "first",
        AssetKind::TileSet,
        &tiles,
    );
    add(
        &mut manifest,
        &mut files,
        "second",
        AssetKind::TileSet,
        &tiles,
    );
    assert_eq!(
        analyze_project_assets(&manifest, &files)
            .usage
            .visual_allocations,
        8192
    );
    let mut too_many = tiles;
    too_many["tiles"]
        .as_array_mut()
        .unwrap()
        .push(json!(vec![0; 64]));
    too_many["flags"].as_array_mut().unwrap().push(json!(0));
    add(
        &mut manifest,
        &mut files,
        "second",
        AssetKind::TileSet,
        &too_many,
    );
    let report = analyze_project_assets(&manifest, &files);
    assert!(report.usage.visual_bytes < 1024 * 1024);
    assert_eq!(report.diagnostics[0].field, "$.visualAllocations");
    assert!(report.diagnostics[0].message.ends_with("over by 1"));
}

#[test]
fn display_font_and_dense_tracker_shapes_are_validated_without_panics() {
    let (mut manifest, mut files) = project();
    manifest.display = Some("assets/display.json".to_owned());
    for remap in [vec![0; 31], vec![0; 256], vec![256; 32]] {
        files.insert(
            "assets/display.json".to_owned(),
            serde_json::to_vec(&json!({"revision":2,"kind":"display","remap":remap,"raster":[]}))
                .unwrap(),
        );
        let report = analyze_project_assets(&manifest, &files);
        assert_eq!(
            report.diagnostics[0].field,
            if remap.len() == 32 {
                "$.remap[0]"
            } else {
                "$.remap"
            }
        );
    }
    let mut font = json!({"revision":1,"kind":"font","glyphWidth":1,"glyphHeight":1,"baseline":0,"advanceX":1,"advanceY":1,"missingGlyph":63,"glyphs":[{"code":63,"pixels":[1]}]});
    assert!(single(AssetKind::Font, &font).is_valid());
    font["glyphs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"code":63,"pixels":[0]}));
    assert!(!single(AssetKind::Font, &font).is_valid());
    for value in [
        json!(null),
        json!([]),
        json!({"revision":1,"kind":"music","framesPerRow":1,"loop":true,"order":["A"],"patterns":{"A":{"rows":[[null]]}}}),
    ] {
        assert!(!single(AssetKind::Music, &value).is_valid());
    }
}

#[test]
fn missing_files_and_diagnostic_floods_are_explicit_and_bounded() {
    let (mut manifest, files) = project();
    for index in 0..129 {
        manifest.assets.insert(
            format!("missing{index:03}"),
            ProjectAsset {
                kind: AssetKind::Sprite,
                path: format!("assets/missing{index:03}.json"),
            },
        );
    }
    let report = analyze_project_assets(&manifest, &files);
    assert_eq!(report.diagnostics.len(), 128);
    assert!(report.diagnostics_truncated);
    assert_eq!(report.require_valid().unwrap_err().code, "M014063");
}
