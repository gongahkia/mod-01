use modl_core::{
    analyze_project_model,
    tooling::{ProjectModel, RelationshipKind},
};
use std::collections::BTreeMap;

const METADATA: &str = "format = 2\nlanguage = \"MODL/1\"\nid = \"author.model\"\ntitle = \"MODEL\"\nauthor = \"@author\"\nversion = \"1.0\"\nentry = \"src/main.modl\"\nupdate_rate = 60\n[assets.hero]\nkind = \"sprite\"\npath = \"assets/hero.m01g\"\n";
const MAIN: &str = "import src.math as math\nstate score: Int = 0\ntask pulse():\n  score += 1\n  wait 1f\non update:\n  score = math.step(score)\n  start pulse()\non draw:\n  sprite(#hero, score, 0)\n";
const MATH: &str = "state score: Int = 1\nfn step(value: Int) -> Int:\n  return value + score\n";

fn files(main: &str, math: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("src/main.modl".to_owned(), main.as_bytes().to_vec()),
        ("src/math.modl".to_owned(), math.as_bytes().to_vec()),
    ])
}

fn source_id(model: &ProjectModel, path: &str) -> modl_core::FileId {
    model
        .sources
        .iter()
        .find(|source| source.path == path)
        .unwrap()
        .id
}

#[test]
fn whole_game_graphs_scope_overlapping_ids_and_keep_invalid_child_diagnostics_and_crlf_offsets() {
    let mut main =
        modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    let code = "state score: Int = 0\r\nfn next(value: Int) -> Int:\r\n  return value + score\r\non update:\r\n  score = next(score)\r\non start:\r\n  assert false, \"static analysis never executes this\"\r\non draw:\r\n  clear(1)\r\n";
    main.files
        .insert(main.manifest.entry.clone(), code.as_bytes().to_vec());
    let mut child = main.clone();
    child.manifest.id = "graph-child".to_owned();
    let source = |child: &modl_core::WorkcartProject| {
        modl_core::workcart::encode_workcart_with_bundle(
            &main.manifest,
            &main.files,
            &[],
            &[],
            &[modl_core::bundle::BundleSource {
                alias: "arena".to_owned(),
                source: modl_core::encode_workcart(&child.manifest, &child.files, &[], &[])
                    .unwrap(),
            }],
        )
        .unwrap()
    };
    let graph = modl_core::tooling::analyze_game_model(&source(&child)).unwrap();
    assert_eq!(graph.members.len(), 2);
    assert_eq!(graph.members["arena"].cartridge_id, "graph-child");
    for member in graph.members.values() {
        let model = &member.model;
        assert!(model.diagnostics.is_empty());
        let file = source_id(model, "src/main.modl");
        assert_eq!(model.source(file).unwrap().text, code);
        let span = model
            .definition_at(
                file,
                u32::try_from(code.rfind("next(score)").unwrap()).unwrap(),
            )
            .unwrap();
        assert_eq!(&code[span.start as usize..span.end as usize], "next");
        assert!(
            model
                .edges
                .iter()
                .any(|edge| edge.kind == RelationshipKind::Call)
        );
    }
    assert_eq!(
        graph.members["main"].model.sources[0].id,
        graph.members["arena"].model.sources[0].id
    );
    child.files.insert(
        child.manifest.entry.clone(),
        b"on draw:\n  missing_function()\n".to_vec(),
    );
    let graph = modl_core::tooling::analyze_game_model(&source(&child)).unwrap();
    assert!(graph.members["main"].model.diagnostics.is_empty());
    let child_model = &graph.members["arena"].model;
    assert!(!child_model.diagnostics.is_empty());
    for diagnostic in &child_model.diagnostics {
        assert!(child_model.source(diagnostic.primary.span.file).is_some());
    }
    child.manifest.id.clone_from(&main.manifest.id);
    assert_eq!(
        modl_core::tooling::analyze_game_model(&source(&child))
            .unwrap_err()
            .code,
        "M014071"
    );
}

#[test]
fn resolves_original_cross_module_names_and_graph_relationships_without_executing() {
    let model = analyze_project_model(METADATA, &files(MAIN, MATH)).unwrap();
    assert!(model.diagnostics.is_empty(), "{:?}", model.diagnostics);
    let main = source_id(&model, "src/main.modl");
    let math = source_id(&model, "src/math.modl");
    let definition = model
        .definition_at(
            main,
            u32::try_from(MAIN.find("math.step").unwrap() + 6).unwrap(),
        )
        .unwrap();
    assert_eq!(definition.file, math);
    assert_eq!(
        &MATH[definition.start as usize..definition.end as usize],
        "step"
    );
    assert_eq!(
        model
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "score")
            .count(),
        2
    );
    assert!(
        !model
            .symbols
            .iter()
            .any(|symbol| symbol.name.starts_with("__m"))
    );
    let local = model
        .definition_at(math, u32::try_from(MATH.rfind("value").unwrap()).unwrap())
        .unwrap();
    assert_eq!(local.start as usize, MATH.find("value").unwrap());
    for kind in [
        RelationshipKind::Call,
        RelationshipKind::Read,
        RelationshipKind::Write,
        RelationshipKind::Start,
        RelationshipKind::Asset,
    ] {
        assert!(model.edges.iter().any(|edge| edge.kind == kind), "{kind:?}");
    }
    for edge in &model.edges {
        assert!(model.nodes.iter().any(|node| node.id == edge.from));
        assert!(model.nodes.iter().any(|node| node.id == edge.to));
        assert!(
            model
                .sources
                .iter()
                .any(|source| source.id == edge.span.file)
        );
    }
    assert_eq!(
        model
            .nodes
            .iter()
            .find(|node| node.id == "asset:hero")
            .unwrap()
            .kind,
        "Sprite"
    );
}

#[test]
fn restores_exact_authored_crlf_offsets_and_original_module_type_diagnostics() {
    let main = MAIN.replace('\n', "\r\n");
    let math = MATH.replace('\n', "\r\n");
    let model = analyze_project_model(METADATA, &files(&main, &math)).unwrap();
    assert!(model.diagnostics.is_empty());
    let definition = model
        .definition_at(
            source_id(&model, "src/main.modl"),
            u32::try_from(main.find("math.step").unwrap() + 6).unwrap(),
        )
        .unwrap();
    assert_eq!(
        &math[definition.start as usize..definition.end as usize],
        "step"
    );
    let invalid = math.replace("value + score", "value + true");
    let model = analyze_project_model(METADATA, &files(&main, &invalid)).unwrap();
    let diagnostic = model
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "M013102")
        .unwrap();
    assert_eq!(
        diagnostic.primary.span.file,
        source_id(&model, "src/math.modl")
    );
    let span = diagnostic.primary.span;
    assert!(invalid[span.start as usize..span.end as usize].contains("true"));
    assert!(
        model.edges.is_empty(),
        "invalid IR is not presented as an executable graph"
    );
}

#[test]
fn syntax_errors_remain_clickable_in_their_own_file_during_incomplete_edits() {
    let invalid = "fn step(value: Int) -> Int:\n  return (\n";
    let model = analyze_project_model(METADATA, &files(MAIN, invalid)).unwrap();
    assert!(!model.diagnostics.is_empty());
    for diagnostic in &model.diagnostics {
        assert_eq!(
            diagnostic.primary.span.file,
            source_id(&model, "src/math.modl")
        );
        assert!((diagnostic.primary.span.end as usize) <= invalid.len());
    }
    assert!(model.nodes.is_empty());
}
