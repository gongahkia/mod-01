# Security boundaries

MOD-01 cartridges are untrusted inputs, but V1 is not a claim of process-level isolation.
The practical boundary combines a restricted source language, compiler-owned generation, a
disposable Web Worker, a validated message protocol, deterministic budgets, and browser origin
controls.

## Cartridge capabilities

MODL/1 has no syntax for raw JavaScript, host values, dynamic code, ambient globals, arbitrary
imports, DOM access, network access, wall-clock time, or host persistence. Named console operations
lower to a fixed dispatcher. Unknown calls fail at the nearest runtime boundary.

Generated JavaScript runs in a dedicated module worker. Before evaluating it, the worker removes or
shadows `Date`, network constructors, browser storage, `crypto`, `navigator`, `eval`, `Function`, and
`Math.random` where the browser permits those properties to be replaced. Cartridge module URLs must
use the host-created `blob:` scheme. Messages in both directions are structurally validated.

The worker receives cloned input frames, its own validated 8 KiB save image, bounded asset bytes and
the read-only canonical cartridge image. It returns resolved indexed pixels, deterministic PCM,
commands and complete committed save images. It does not receive IndexedDB handles, the Studio
project database, another cartridge ID, or another cartridge's save block. The trusted host applies
commits through a capability bound to the immutable current cartridge ID.

## Resource controls

The compiler charges synthetic work units at function entries, loop back-edges, task transitions,
integer/capacity operations, allocations, and console calls. Exceeding the per-frame limit throws a
source-spanned `M019001` fault. Tasks are explicit serializable program-counter machines and do not
receive promises. The host also applies a response deadline and terminates a worker that stops
responding, covering malformed or non-compiler JavaScript that never reaches an inserted check.

These controls protect studio responsiveness and deterministic replay semantics. They do not defend
against a compromised browser engine, same-origin side channels outside the worker model, or a user
manually substituting arbitrary JavaScript for compiler output.

Standalone HTML exports embed the canonical cartridge and a generated dependency-free host. Its
inline worker is built from the same locked-down Worker and production console core as Studio,
including all Hardware Revision 1 permissions and fixed ceilings. It has no project-database
capability. The standalone file is intentionally self-contained and therefore uses inline script
and `blob:` workers;
it does not claim the same deployable Content Security Policy surface as the hashed Studio build. A
user who edits the exported HTML can also edit its runtime and must not treat it as a signed format.

The native headless adapter does not execute an imported cartridge's archived JavaScript. Rust
first performs bounded canonical decode, reconstructs its source-visible project, recompiles it with
the authoritative compiler and sends that output plus bounded data to the embedded production-core
Node host. MODL has no path to Node globals; malformed host requests reject before boot. Node is a
documented CLI runtime dependency, not a cartridge capability.

The native desktop Studio also discards archived build JavaScript and recompiles imported source.
Its console host runs in a child process with Node's permission model enabled, filesystem read
access limited to the generated host bundle, no inherited user environment apart from process
essentials, and no child-process permission. Rust limits each response to 16 MiB and terminates a
host that fails to answer within five seconds. A packaged app includes its own Node executable.
These controls limit accidental host access and contain a stalled runtime; they are not an OS
sandbox for arbitrary attacker-supplied JavaScript.

The PWA service worker caches only same-origin GET requests and an exact build-generated inventory.
Its cache revision covers hashed bundles and bundled-cartridge bytes. It adds offline availability,
not a new cartridge capability or a trust boundary.

The production Studio document carries an explicit policy: same-origin scripts/styles/fonts/images
and connections only, no objects or form submission, and `blob:` only for the inlined locked-down
Worker and compiler-created cartridge modules, with the narrowly required Wasm execution permission.
Imported JavaScript is never executed. Normal cartridge run/capture/export performs no request.
Folder access is available only after the author invokes `FOLDER` and the browser grants
a directory handle; handles stay in the current Studio session and are never passed to a cartridge.

All untrusted formats preflight packed/decompressed byte length, entry/file count, dimensions,
frame/count/depth limits and canonical paths before mutation. Cartridge archives reject traversal,
duplicate or unsorted entries, noncanonical compression, trailing data and integrity failure. PNG
chunks, visual/font/map/audio JSON, fragments, replay and save containers have bounded strict
decoders. Imported archived JavaScript is never executed: source is reconstructed and passed through
the authoritative compiler, including headless and source-only carts. Save imports also bind the
immutable cartridge ID and verify their CRC before transactional replacement.

## Verified evidence

The production Vite worker bundle is exercised in pinned Firefox and Chromium through Playwright:

- compiler-produced code completed a frame and returned one validated draw command;
- the worker audit found none of the denied globals exposed and no `Math.random`;
- an infinite MODL `while true` loop stopped with source-spanned `M019001` while the page remained
  responsive;
- the display remained unclipped and crisp at exact 2x and 3x viewports.

The query routes `?sandbox-test=normal`, `?sandbox-test=capabilities`, and
`?sandbox-test=runaway` reproduce those checks after `pnpm dev`.
