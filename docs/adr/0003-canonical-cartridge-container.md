# ADR 0003: Canonical cartridge container

Status: accepted for the sole current M01C/2 encoding.

Use a sorted MOD-01 container with SHA-256 inventories and canonical PackBits-style
RLE. Store exact authored workcart source once, canonical metadata and optional
redundant release build products. Always record and verify compiled-program identity.
There are no timestamps, host paths or platform metadata.

The 1 MiB packed limit and 16 MiB expansion bound are checked before use.
Alternate packets, unsorted/unsafe paths, noncanonical JSON, source/metadata
disagreement, unexpected entries, hash mismatches and trailing bytes are errors.
No historical encoding or migration path is maintained.
See [CARTRIDGE_FORMAT.md](../CARTRIDGE_FORMAT.md) for the complete contract.
