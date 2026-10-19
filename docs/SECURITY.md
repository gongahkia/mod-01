# Security boundaries

MOD-01 runs native Studio, CLI and standalone desktop games. There is no browser
origin, service worker, DOM, webview, worker capability audit or CSP boundary.

## Cartridge code and host capabilities

MODL has no syntax for arbitrary JavaScript, Node values, ambient globals, dynamic
imports, filesystem/network access or wall-clock time. Named console operations
lower to a fixed dispatcher. Unknown calls fault rather than reaching a host API.

Native Studio and the CLI reconstruct imported source-visible cartridges and
compile their MODL with the shared Rust compiler. They do not execute imported
archived JavaScript directly. The native game surface uses Studio's same project
loader and compiler. Source-only cartridges also verify the packed compiled
identity before use by the relevant core load operation.

The production Node host accepts compiler-generated code and bounded device data.
It is an internal host protocol, not an API for accepting arbitrary JavaScript
from a remote caller. Runtime source/module substitution by a local user is
outside the restricted-language claim.

## Process and resource controls

The native bridge launches a disposable Node child with its permission model
enabled and filesystem read permission limited to the host bundle. Environment
inheritance is restricted to process essentials; child-process permission is not
granted. The bridge bounds each response to 16 MiB, imposes a five-second response
deadline, and kills a stalled or faulted process. Packaged games include Node;
development and the external CLI headless workflow require a local Node install.

The bridge resolves host symlinks, then uses `dunce`'s safe Windows path
simplification for both Node's entry and exact-file read grant. Rust's verbatim
`\\?\` spellings are not interchangeable with Node permission keys. Reserved
or otherwise ambiguous names are not blindly stripped, and no directory/all-file
permission fallback is added. A real child-host probe checks spaces, Unicode and
`#` in the host path while confirming an adjacent file remains unreadable.

These are defence-in-depth controls and a responsiveness boundary. Node's
permission model is not an OS sandbox against malicious arbitrary JavaScript.
Do not treat imported native executables, replacement runtimes or a compromised
compiler/Node/OS as safe.

Generated MODL charges synthetic work at loop back-edges, function/task
transitions, allocations and console operations. Exceeding the frame ceiling
faults at a source span. Tasks are serializable state machines, not promises.
Math/text/graphics operations have additional bounded costs. Snapshots accept
only the current device/scheduler schema, with transactional restore on failure.

## Files, saves and distribution

Cartridge decode preflights archive size/count, canonical paths, compression and
integrity. PNG, asset, replay and save decoders validate their data before use.
Cartridges receive only their own validated assets, ROM, inputs and save image.
Rust, not cartridge code, chooses save paths from validated cartridge identities
and applies committed save bytes. Save import checks identity and checksum;
recovery copies remain local.

Native export validates target binary headers and matching architectures, refuses
an existing destination, and bundles the exact packed cartridge and runtime.
An I/O failure may leave a partial **new** export directory. Packages are not
cryptographically signed/notarized by this tool; the publisher must handle OS
signing/trust requirements. Redistribution includes Node and visual-asset
notices. A package's native binaries remain trusted code, not cartridge data.

Tests exercise actual release/debug compiler output, bounded faults, atomic
snapshot restore and the native child bridge. The native package verifier runs
from an unrelated working directory with an empty PATH and compares cartridge
bytes and deterministic frames. Graphical/input/audio QA remains a separate
requirement in the acceptance ledger.
