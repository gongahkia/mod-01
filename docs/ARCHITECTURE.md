# Architecture

MOD-01 has a Rust native desktop Studio and a maintained static browser Studio. They share the
Rust compiler, cartridge format, and deterministic TypeScript console core.

## Package ownership

- `crates/modl-core`: MODL syntax, semantic analysis, typed IR, code generation, debug metadata,
  deterministic cartridge encoding, standalone HTML export, formatter, and language-service
  analysis.
- `crates/modl-wasm`: a deliberately narrow WebAssembly boundary over `modl-core`.
- `crates/mod01-cli`: native command family and stdio language server, both using `modl-core`.
- `crates/mod01-native`: native egui/eframe Studio, layered sprite editor, desktop input/audio,
  project persistence, and a bounded child-process bridge to the console runtime.
- `packages/runtime`: deterministic hardware model, worker protocol, indexed graphics, input, audio,
  persistence adapters, debugger, replay, and export support.
- `apps/studio`: 240x144 shell and integrated tools. It consumes the runtime and compiler bridge but
  owns neither compiler rules nor cartridge execution.

Dependencies point inward: studio -> runtime/WASM bridge -> compiler core. Cartridge code runs only
in a dedicated worker and communicates through a versioned, validated message protocol. It never
receives persistence handles or DOM objects.

The Worker delegates scheduling, console dispatch, work accounting, graphics, audio, maps, saves
and debug traces to `packages/runtime/src/console-runtime.ts`. That core validates its complete
configuration with the same schema as the Worker protocol, rejects budgets above 50,000, and decodes a
bounded source asset bank; map queries and drawing use the same visual store. Commands execute in
program order, then frame completion resolves indexed scanout and renders deterministic PCM.
The page presents those pixels/samples and handles input and storage; it no longer re-executes
cartridge graphics or audio. Tests drive this same core with native-compiled MODL and compare
actual alpha browser recordings. `MemoryBus` aliases work RAM and the actual graphics/raster
storage with transactional byte/word/copy/fill access. Its frozen layout is documented in HARDWARE.
The browser Worker, the embedded offline standalone Worker, and the
native CLI's Node headless adapter all drive this same core rather than reimplementing console calls.

## Data flow

MODL source is tokenized, parsed, resolved to stable symbol IDs, type-checked against a typed asset
catalog, and lowered to a serializable structured IR. That IR is instrumented and generated as
compact JavaScript plus source maps; the same IR produces debug and release output.
Projects remain Git-friendly directories; packing creates a canonical, content-addressed `.m01c`
artifact containing original source and compiled output.

Standalone export deliberately reuses the pack/decode boundary: verified archive entries and the
exact canonical ROM are embedded with a generated host whose inline Worker is built from the same
`sandbox-worker.ts` and production core as Studio. The generated player is checked in so Rust/Wasm
exports are reproducible without a JavaScript build step; the runtime package build regenerates it.
The headless host is generated from `headless.ts`, embedded in the native CLI, and accepts only a
bounded JSON request prepared by Rust. Imported `.m01c` files are unpacked and recompiled from their
source before execution, so their archived build entry is never trusted as executable input.
The native Studio compiles source with `modl-core`, then passes the generated program, verified ROM,
and source-visible assets to `native-host.mjs` over line-oriented JSON. Node runs with its permission
model enabled, an empty environment apart from process essentials, and read access only to the host
bundle. Rust enforces a response size and time limit, converts indexed pixels to a native texture,
plays PCM through CPAL, and gathers up to four controllers through gilrs. Distribution bundles the
Node executable alongside the Rust app, with no webview or browser Worker.
The Vite build emits a service worker from the final hashed asset inventory, so offline caching
tracks the actual build instead of a handwritten filename list.

Debug output adds source probes and routine enter/leave hooks without changing typed IR. The worker
returns bounded traces and serializable state/task inspection only when debug mode is requested.
The revision-6 Worker snapshot includes the scheduler, saves and pending writes, indexed-framebuffer,
synthesizer and retained bus state. Restore validates all components and rolls back on a device-reference failure.
The Studio journal still retains its revision-1 wrapper, now populated from that authoritative
snapshot; recorded inputs and canonical fingerprints provide deterministic rewind with explicit
divergence detection. Raw legacy Worker snapshots restore only their original scheduler/save fields;
they do not contain graphics/audio. Public revision-0 replay records migrate into the bounded
revision-1 envelope without replacing the original input. The debugger does not grant cartridges
DOM, persistence, or network capabilities.

Visual assets now occupy one packed 128 KiB image. Byte views back pixels/flags and explicit
little-endian DataViews back map cells; the renderer and map query API read those views directly.
Display defaults also occupy their charged bytes, with frame-start latching into live registers.
Read-only descriptors expose allocation order and exact sizes. Legacy revision-3 bus snapshots
migrate the new visual region from source, while current snapshots retain all mutable visual bytes.
Read-only controller/pointer MMIO encodes the scheduler's actual current/previous input frames;
there is no shadow input buffer to synchronize or independently restore. Core input validation
precedes graphics resets; direct `DeterministicMachine` callers are validated at its frame boundary too.
System MMIO likewise encodes the real scheduler, RNG and work ledger. Revision-2 machine snapshots
retain boot/counter/cadence/accounting and terminal fault state, with explicit defaults for legacy
revision-1 machines. Fault retries are rejected before any per-frame device reset. Current snapshots
are completed-frame or fault boundaries; they do not claim resumable source-statement execution.
Writable audio MMIO uses side-effect-free preparation and a commit after all bus targets validate.
It updates the existing voice/tracker objects directly; their existing synth snapshot remains the
single retained representation. Scanout and audio mixing run inside the scheduler's output phase
before its frame counter advances, so a device failure is captured by the same terminal fault latch.
Generated factories do not execute global initializers. The runtime first attaches the machine and
devices, then `boot()` runs global initialization and `on start` in one start-phase work budget. This
keeps initialization-time console calls inside the same execution and device boundary as callbacks.

Architecture decisions live in [`docs/adr`](adr/). The V1 contract in [PRODUCT.md](PRODUCT.md),
[LIMITS.md](LIMITS.md), and [V1_RELEASE_EVIDENCE.md](V1_RELEASE_EVIDENCE.md) takes precedence when a
documented implementation detail conflicts with this overview.
