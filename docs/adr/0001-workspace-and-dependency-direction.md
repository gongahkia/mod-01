# ADR 0001: Workspace and dependency direction

Status: accepted, current native architecture.

Cargo owns compiler, formatter, workcart, packing, analysis, CLI and native UI.
The pnpm runtime package owns deterministic console devices and the native/headless
Node hosts. Studio and packaged games use the same child-process bridge; CLI
headless uses the same production core. No Wasm, browser or webview adapter exists.

Cross-boundary messages are bounded and validated. UI code cannot patch compiler
semantics or implement another emulator. Runtime device tests do not require
native chrome. Build runtime host bundles before compiling binaries that embed them.
