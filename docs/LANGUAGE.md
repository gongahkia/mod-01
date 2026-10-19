# MODL/1 language contract

MODL/1 is MOD-01's ASCII-only, statically typed, indentation-sensitive cartridge
language. The complete syntax, types, resolution, initialization, tasks, work
accounting and diagnostics are maintained in [MODL.md](MODL.md), with bounded
math/text/bit helpers in [STANDARD_LIBRARY.md](STANDARD_LIBRARY.md).

Each module has a top-level namespace. Declarations are public by default;
`private` hides imported members. Dotted imports expose qualified members,
initialize dependencies before importers, and reject cycles and dependency
callbacks. No classes, inheritance, reflection, macros, ambient host JavaScript
or arbitrary dynamic module loading are available.

Studio, CLI, formatter, compiler explorer and LSP use the same Rust lexer,
parser, type analysis and linker. Executable conformance fixtures are the
authority when implementation and prose diverge.
