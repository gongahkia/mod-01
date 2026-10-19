# ADR 0002: Determinism and native process boundary

Status: accepted, current native architecture.

Generated MODL executes in a disposable Node child process. Rust recompiles
cartridge source and verifies its compiled identity instead of trusting imported
JavaScript. The host accepts bounded input/assets/ROM/save data; time, RNG,
scheduling, work and replay are console-owned state.

The compiler inserts deterministic work checks at function entries, loop edges,
task transitions, allocations and console operations. The bridge imposes response
limits/deadlines and terminates a stalled child. Node permissions add defence in
depth but are not an OS sandbox for arbitrary malicious JavaScript.

Every observable device state is included in validated current-schema snapshots.
Unsupported schemas are rejected rather than migrated. See [SECURITY.md](../SECURITY.md)
for the precise trust boundary.
