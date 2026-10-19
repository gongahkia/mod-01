# Architecture

MOD-01 is a native authoring/player product with one shared compiler and console
core. There is no browser frontend or browser distribution target.

```text
Native Studio / external editor
          |
          v
    modl-core (Rust)
    parser -> typed IR -> release/debug generation
    authoring / validation / canonical cartridge
          |
          +-- mod01 CLI: check, build, test, pack, PNG/native export
          |
          v
    shared console core (TypeScript, bundled for Node)
    deterministic scheduler / MMIO / graphics / audio / input / saves
          |
          +-- native child host -> Rust display/audio/save bridge
          +-- headless host -> reproducible traces and cartridge tests
```

The native game surface and Studio are two presentations of the same executable.
They use the same project loader, compiler, runtime bridge and local save identity.
A native distribution carries the executable, cartridge, Node, shared host and
notices together. Its default cartridge is resolved relative to the executable,
not the working directory.

## Compiler and data

`crates/modl-core` owns lexing, parsing, name/type resolution, typed IR, code
generation, source maps, workcart encoding, generation recipes and packed/PNG
cartridge encoding. Release and resumable debug code are generated from the same
typed IR. Callbacks and tasks have explicit state; there is no hidden scene engine.

Workcarts are source-visible authoring documents. Packed cartridges carry the
authored source and runtime assets. Presentation metadata is not trusted as a
replacement for canonical cartridge integrity. Native imports reconstruct and
compile source instead of executing an imported archived JavaScript build.

## Console and host

`packages/runtime/src/console-runtime.ts` is the production device dispatcher.
`machine.ts` owns deterministic frame/update scheduling, work budgets, input
edges, RNG and source-debug stepping. Graphics/audio stores share real bounded
MMIO bytes; memory reads and drawing operate on the same device data.
Snapshots contain the current machine and devices and restore transactionally;
obsolete schemas are rejected.

Math uses specified arithmetic algorithms, text/conversions are bounded and
locale-free, and bit operations use explicit word/shift rules. These are console
calls, not access to JavaScript globals. See [standard library](STANDARD_LIBRARY.md).

`native-host.ts` exposes bounded line-delimited process messages.
`headless.ts` produces frame/state/audio/save hashes through the same device core.
Both are built as Node bundles; the build has no browser player entry point.

`crates/mod01-native` owns egui, native keyboard/gamepad/pointer input, pixel
presentation, native PCM output and local file/save operations. The child process
has a response-size limit and deadline. Node permissions are defence in depth,
not an OS security sandbox; see [security](SECURITY.md).

## Evidence

Runtime tests exercise devices and bounded failure paths. Compiler tests verify
syntax/types/code generation. Conformance tests compile MODL and run release/debug
code in the actual shared, native and headless hosts. Native package verification
compares canonical cartridge bytes and gameplay from a fresh package with no PATH
dependency. The OS packaging matrix applies the same process on Windows/macOS/Linux.
Graphical interaction QA and outstanding scope are tracked separately in the
[acceptance ledger](NATIVE_IMPLEMENTATION.md).
