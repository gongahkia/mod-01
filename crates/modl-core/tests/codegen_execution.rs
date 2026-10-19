use std::{fs, process::Command};

use modl_core::{AssetCatalog, CompileMode, FileId, SourceFile, compile};

fn execute_source(label: &str, source: &str) -> std::process::Output {
    let source = SourceFile::new(FileId(0), format!("{label}.modl"), source);
    let output = compile(&source, &AssetCatalog::default(), CompileMode::Release);
    assert!(
        output.analysis.diagnostics.is_empty(),
        "{:#?}",
        output.analysis.diagnostics
    );
    let program = output.generated.expect("valid source generates JavaScript");
    let script = format!(
        r#"{}
let workUnits=0;
const api={{
  work(units){{workUnits+=units;if(workUnits>100000)throw new Error("M019001: budget");}},
  call(){{throw new Error("unexpected console API call");}},
  fault(code,message){{throw new Error(`${{code}}: ${{message}}`);}}
}};
const cartridge=createCartridge(api);
cartridge.start();
process.stdout.write(JSON.stringify(cartridge.snapshot()));
"#,
        program.javascript
    );
    let path =
        std::env::temp_dir().join(format!("modl-codegen-{label}-{}.mjs", std::process::id()));
    fs::write(&path, script).expect("temporary generated module writes");
    let result = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js executes generated module");
    fs::remove_file(path).expect("temporary generated module removes");
    result
}

fn execute(mode: CompileMode) -> serde_json::Value {
    let source = SourceFile::new(
        FileId(0),
        "semantic-equivalence.modl",
        r"state score: Int = 0
task tick():
  var index = 0
  while index < 3:
    wait 1f
    score += 1
    index += 1
on start:
  start tick()
on update:
  score += 10
",
    );
    let output = compile(&source, &AssetCatalog::default(), mode);
    assert!(
        output.analysis.diagnostics.is_empty(),
        "{:#?}",
        output.analysis.diagnostics
    );
    let program = output.generated.expect("valid source generates JavaScript");
    let script = format!(
        r#"{}
let workUnits=0;
const api={{
  work(units){{workUnits+=units;if(workUnits>100000)throw new Error("budget");}},
  call(){{throw new Error("unexpected console API call");}},
  fault(code,message){{throw new Error(`${{code}}: ${{message}}`);}},
  probe(){{}},enter(){{}},leave(){{}}
}};
const cartridge=createCartridge(api);
const complete=callback=>{{for(;;){{const step=callback();if(step===undefined||step.done)break;}}}};
complete(()=>cartridge.start());
for(let frame=0;frame<6;frame+=1)complete(()=>cartridge.update());
process.stdout.write(JSON.stringify({{snapshot:cartridge.snapshot(),workUnits}}));
"#,
        program.javascript
    );
    let suffix = match mode {
        CompileMode::Release => "release",
        CompileMode::Debug => "debug",
    };
    let path =
        std::env::temp_dir().join(format!("modl-codegen-{}-{suffix}.mjs", std::process::id()));
    fs::write(&path, script).expect("temporary generated module writes");
    let result = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js executes generated module");
    fs::remove_file(path).expect("temporary generated module removes");
    assert!(
        result.status.success(),
        "generated module failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).expect("generated module prints JSON")
}

#[test]
fn runtime_globals_execute_at_start_not_factory_construction() {
    for mode in [CompileMode::Release, CompileMode::Debug] {
        let source = SourceFile::new(
            FileId(0),
            "boot.modl",
            "state value: Int = mem_read(0)\non start:\n  value += 1\n",
        );
        let output = compile(&source, &AssetCatalog::default(), mode);
        assert!(
            output.analysis.diagnostics.is_empty(),
            "{:#?}",
            output.analysis.diagnostics
        );
        let program = output
            .generated
            .expect("valid boot source generates JavaScript");
        let script = format!(
            r#"{}
let workUnits=0;
const calls=[];
const api={{
  work(units){{workUnits+=units;}},
  call(name,args){{calls.push([name,args]);return 7;}},
  fault(code,message){{throw new Error(`${{code}}: ${{message}}`);}},
  probe(){{}},enter(){{}},leave(){{}}
}};
const cartridge=createCartridge(api);
if(workUnits!==0||calls.length!==0||Object.keys(cartridge.snapshot().state).length!==0)throw new Error("factory executed globals");
for(;;){{const step=cartridge.start();if(step===undefined||step.done)break;}}
if(Object.values(cartridge.snapshot().state)[0]!==8||calls.length!==1||calls[0][0]!=="mem_read"||workUnits===0)throw new Error("boot did not initialize globals");
"#,
            program.javascript
        );
        let path =
            std::env::temp_dir().join(format!("modl-codegen-boot-{}.mjs", std::process::id()));
        fs::write(&path, script).expect("temporary boot module writes");
        let result = Command::new("node")
            .arg(&path)
            .output()
            .expect("Node.js executes boot module");
        fs::remove_file(path).expect("temporary boot module removes");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn release_and_debug_outputs_are_semantically_equivalent() {
    let release = execute(CompileMode::Release);
    let debug = execute(CompileMode::Debug);
    assert_eq!(release, debug);
    assert_eq!(
        release["snapshot"]["state"]
            .as_object()
            .and_then(|state| state.values().next())
            .and_then(serde_json::Value::as_i64),
        Some(63)
    );
    assert_eq!(
        release["snapshot"]["tasks"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn enum_equality_survives_snapshot_restore_and_compares_nested_payloads() {
    let source = SourceFile::new(
        FileId(0),
        "enum-replay.modl",
        r#"record Payload:
  values: [Int, 2]
enum Mode:
  Title
  Play
  Data(Payload)
state mode: Mode = Mode.Title
state packet: Mode = Mode.Data(Payload([2, 7]))
state ticks: Int = 0
on update:
  assert mode == Mode.Title, "restored title"
  assert mode != Mode.Play, "different variant"
  assert packet == Mode.Data(Payload([2, 7])), "equal payload"
  assert packet != Mode.Data(Payload([2, 8])), "different payload"
  ticks += 1
"#,
    );
    for mode in [CompileMode::Release, CompileMode::Debug] {
        let output = compile(&source, &AssetCatalog::default(), mode);
        assert!(
            output.analysis.diagnostics.is_empty(),
            "{:#?}",
            output.analysis.diagnostics
        );
        let program = output.generated.expect("enum fixture compiles");
        let script = format!(
            r#"{}
const api={{work(units){{if(units<0)throw new Error("negative work");}},call(){{throw new Error("unexpected API");}},fault(code,message){{throw new Error(`${{code}}: ${{message}}`);}}}};
const cartridge=createCartridge(api);
cartridge.start();
const before=cartridge.snapshot();
cartridge.update();
const expected=JSON.stringify(cartridge.snapshot());
cartridge.restore(before);
cartridge.update();
if(JSON.stringify(cartridge.snapshot())!==expected)throw new Error("replay differs");
"#,
            program.javascript
        );
        let result = Command::new("node")
            .args(["--input-type=module", "-e", &script])
            .output()
            .expect("Node.js executes enum regression");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn options_and_fixed_collection_indexes_execute_with_runtime_guards() {
    let valid = execute_source(
        "option-collection",
        r"state saved: Option[Int] = none
state values: List[Int, 2] = []
on start:
  values[0] = unwrap_or(saved, 7)
  saved = some(5)
  values[1] = unwrap_or(saved, 0)
",
    );
    assert!(
        valid.status.success(),
        "generated module failed:\n{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let snapshot: serde_json::Value =
        serde_json::from_slice(&valid.stdout).expect("generated module prints JSON");
    let values = snapshot["state"]
        .as_object()
        .expect("state is an object")
        .values()
        .collect::<Vec<_>>();
    assert!(
        values
            .iter()
            .any(|value| value.as_array() == Some(&vec![7.into(), 5.into()]))
    );
    assert!(
        values
            .iter()
            .any(|value| value["value"].as_i64() == Some(5))
    );

    let outside = execute_source(
        "collection-bounds",
        "state values: List[Int, 1] = []\non start:\n  values[1] = 9\n",
    );
    assert!(!outside.status.success());
    assert!(String::from_utf8_lossy(&outside.stderr).contains("M019007"));

    let oversized_range = execute_source(
        "range-budget",
        "state total: Int = 0\non start:\n  for value in 0..1000000:\n    total += value\n",
    );
    assert!(!oversized_range.status.success());
    assert!(String::from_utf8_lossy(&oversized_range.stderr).contains("M019001"));
}

#[test]
fn debug_output_suspends_through_nested_calls_loops_and_tasks() {
    let source = SourceFile::new(
        FileId(0),
        "debug-steps.modl",
        r"state total: Int = 0
fn add_twice(value: Int) -> Int:
  var result = 0
  for index in 0..2:
    result += value
  return result
task tick():
  total += 1
  wait 1f
  total += 1
on start:
  start tick()
on update:
  total += add_twice(3)
",
    );
    let output = compile(&source, &AssetCatalog::default(), CompileMode::Debug);
    assert!(
        output.analysis.diagnostics.is_empty(),
        "{:#?}",
        output.analysis.diagnostics
    );
    let program = output.generated.expect("debug fixture compiles");
    let script = format!(
        r#"{}
const api={{work(){{}},call(){{throw new Error("unexpected API");}},fault(code,message){{throw new Error(`${{code}}: ${{message}}`);}},enter(){{}},leave(){{}}}};
const cartridge=createCartridge(api);
const events=[];
const complete=callback=>{{for(;;){{const step=callback();if(step.done)break;events.push({{event:step.event,stack:structuredClone(cartridge.inspect().callStack)}});}}}};
complete(()=>cartridge.start());
complete(()=>cartridge.update());
process.stdout.write(JSON.stringify({{events,snapshot:cartridge.snapshot()}}));
"#,
        program.javascript
    );
    let result = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .output()
        .expect("Node.js executes debug suspension fixture");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("debug fixture prints JSON");
    let events = result["events"].as_array().expect("events are an array");
    assert!(
        events.len() >= 10,
        "expected statement boundaries: {events:#?}"
    );
    assert!(
        events.iter().any(|entry| entry["stack"]
            .as_array()
            .is_some_and(|stack| stack.len() >= 2)),
        "nested function stack was not live at a suspension: {events:#?}"
    );
    assert!(
        events.iter().any(|entry| entry["stack"]
            .as_array()
            .is_some_and(|stack| stack.iter().any(|frame| frame["name"] == "tick"))),
        "task stack was not live at a suspension: {events:#?}"
    );
    assert_eq!(
        result["snapshot"]["state"]
            .as_object()
            .and_then(|state| state.values().next())
            .and_then(serde_json::Value::as_i64),
        Some(7)
    );
}
