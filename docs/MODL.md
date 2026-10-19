# MODL/1 language reference

MODL/1 (MOD Language, Revision 1) is an ASCII-only, statically typed language for
deterministic MOD-01 cartridges. Newlines terminate statements and indentation defines blocks.
Keywords are lowercase, identifiers are case-sensitive, and indexes and half-open ranges are
zero-based. The compiler and its conformance fixtures are authoritative when this document differs.

The implemented compiler resolves names to stable symbol IDs and lowers valid modules to typed IR.
Project builds link absolute dotted imports such as `import src.math as math` to `src/math.modl`.
Revision 1 gives every module its own top-level namespace, permits the same declaration name in
different modules, and resolves imported members through their alias. It rejects cycles and system
callbacks in dependency modules. The single-file analysis API deliberately reports that imported
members require project analysis.

## Lexical rules

- Indentation uses spaces. The formatter emits two spaces per level; tabs are an error (`M011002`).
- `//` begins a comment. Strings use double quotes and support `\n`, `\t`, `\"`, and `\\` escapes.
- Decimal `Num` literals contain a decimal point. `Int` literals do not.
- Durations use `2f` for frames and `0.5s` for deterministic cartridge seconds.
- `#hero` is a named asset reference, not a string.
- Operators include `+ - * / %`, `& | ^ ~ << >> >>>`, comparisons, `and or not`, assignment and compound assignment.
- [STANDARD_LIBRARY.md](STANDARD_LIBRARY.md) specifies numeric, text and bit helpers and their bounds.
- `0..10` is a half-open range. The end value is excluded.

## Declarations

Persistent mutable state always has an explicit type:

```modl
const MAX_SPEED: Num = 4.5
state score: Int = 0
state recent: List[Int, 16] = []
state ports: [Int, 4] = [0, 0, 0, 0]
```

Top-level declarations are public by default. `pub` may make
that intent explicit; `private` prevents access through an import alias. Visibility applies to
constants, state, functions, tasks, records, and enums. Imports are absolute project paths and
always use qualified member access:

```modl
import src.motion as motion

private const STEP: Int = 2
pub fn advance(x: Int) -> Int:
  return motion.clamp_x(x + STEP)
```

Dependencies initialize before their importers in a deterministic depth-first order, with paths
and imports traversed in source order. A cycle is a `M014008` project diagnostic. Repeated aliases,
missing/private members, and invalid module surfaces are `M014009` diagnostics. Only the entry module
may declare system callbacks. Single-file cartridges need no imports or visibility modifiers.

Locals use inferred types. `let` is immutable and `var` is mutable. Functions declare parameter and
return types. Tasks have typed parameters but no return value and may suspend with `wait`.

```modl
fn add(left: Int, right: Int) -> Int:
  return left + right

task flash(color: Color):
  pal(1, color)
  wait 2f
```

System callbacks are `on start:`, `on update:`, `on draw:`, and
`on raster(line: Int):`. A task begins only through `start task_name(arguments)`.

Records contain typed fields and optional defaults. Enum variants may have typed payloads. `match`
arms begin with `case`; `else` is the wildcard arm.

```modl
record Player:
  pos: Vec2
  lives: Int = 3

enum Mode:
  Title
  Play
  Win(Int)
```

Defaulted record fields must follow required fields and may be omitted from positional construction.
Enum matches must cover every variant or include `else`. Pattern payload bindings have the declared
variant field types.

Enum `==` and `!=` compare the variant and payload values, not host object identity, including after
snapshot restore. Payload records and collections are compared by their stored fields/elements.
Each payload slot and traversed nested field costs one work unit at the comparison's source span;
payload-free variants require no extra work units. Traversal uses the normal frame budget.

## Static types

`Num` is the deterministic numeric path and `Int` is validated for integer-only operations. Integer
literals may initialize `Num`, and palette indices from 0 through 255 may initialize `Color`; other
implicit conversions are rejected. `Bool`, `Text`, `Vec2`, `Rect`, `Controller`, and `Button` are
distinct value types. `Unit` is the no-value function return type.

`Option[T]`, fixed arrays `[T, N]`, and fixed-capacity `List[T, N]` are bounded. `none` constructs an
empty option; `some(value)` preserves the element's static type, `is_some(option)` tests it, and
`unwrap_or(option, fallback)` returns the contained value or a same-typed fallback. A fixed array
literal may initialize a list when its length does not exceed the declared capacity. Capacity is
currently restricted to 1 through 65535. Generated reads and writes validate integer indices against
the declared length/capacity; an uninitialized sparse-list read is also a source-mapped runtime
fault. Indexing `Text` uses Unicode scalar positions and returns the character code and is read-only. General dynamic allocation
is not part of MODL/1.

Asset types are `Sprite`, `Animation`, `TileSet`, `Map`, `Font`, `Sound`, and `Music`. A `#name`
reference obtains its type from the cartridge asset catalog. Missing assets and passing one asset
kind where another is required have distinct diagnostics.

The four `Controller` values are `pad1` through `pad4`. The `Button` values are `up`, `down`, `left`,
`right`, `a`, `b`, `x`, `y`, `l`, `r`, `start_button`, and `menu`. These are typed built-in values,
not strings or integers. The complete implemented console-call surface is in [`API.md`](API.md).
Pointer/touch input is available through `pointer_x()`, `pointer_y()`, `pointer_inside()`,
`pointer_primary()`, and `pointer_secondary()`; the button calls are transition-triggered.

Functions are non-capturing references and can be stored in locals. `let` bindings cannot be
assigned after initialization; `var` and top-level `state` are mutable. Tasks return `Unit`, may
`wait`, and must be launched with a direct `start task_name(...)` call. Ordinary functions and system
callbacks cannot suspend.

Module-level `const` initializers use the compile-time expression subset: primitive literals,
earlier constants, unary numeric/Boolean operators, arithmetic, comparisons, equality, and Boolean
operators. Module-level `assert` uses the same subset and fails compilation when false or undecidable.

Global bindings initialize in linked declaration order at boot, before `on start`, after devices
have been attached. Mutable `state` initializers may call ordinary functions and runtime APIs; their
work and `on start` share one boot budget. Their drawing/audio/save effects use the same live devices
as callbacks. They do not run again on an ordinary frame or when restoring an already booted snapshot.
The generated JavaScript factory only creates the instance; its `start()` initializes globals before
the start callback. A low-level pre-boot snapshot therefore has no initialized global bindings yet.

## Grammar

The following EBNF specifies the accepted syntax. Whitespace between tokens is omitted; `NEWLINE`,
`INDENT`, and `DEDENT` are structural lexer tokens.

```ebnf
module       = { NEWLINE | item }, EOF ;
item         = import | [ visibility ], ( constant | state | function | task | record | enum )
             | callback | assertion ;
visibility   = "pub" | "private" ;
import       = "import", path, [ "as", identifier ], line-end ;
constant     = "const", identifier, [ ":", type ], "=", expression, line-end ;
state        = "state", identifier, ":", type, "=", expression, line-end ;
function     = "fn", identifier, parameters, "->", type, ":", block ;
task         = "task", identifier, parameters, ":", block ;
callback     = "on", ( "start" | "update" | "draw" | "raster" ),
               [ parameters ], ":", block ;
parameters   = "(", [ parameter, { ",", parameter } ], ")" ;
parameter    = identifier, ":", type ;
record       = "record", identifier, ":", NEWLINE, INDENT,
               { identifier, ":", type, [ "=", expression ], line-end }, DEDENT ;
enum         = "enum", identifier, ":", NEWLINE, INDENT,
               { identifier, [ "(", type, { ",", type }, ")" ], line-end }, DEDENT ;
type         = path, [ "[", type-arg, { ",", type-arg }, "]" ]
             | "[", type, ",", integer, "]" ;
type-arg     = type | integer ;
block        = NEWLINE, INDENT, { NEWLINE | statement }, DEDENT ;
statement    = ( "let" | "var" ), identifier, "=", expression, line-end
             | assignable, assign-op, expression, line-end
             | "return", [ expression ], line-end
             | ( "break" | "continue" ), line-end
             | "wait", expression, line-end
             | "start", expression, line-end
             | assertion | if | while | for | match | expression, line-end ;
if           = "if", expression, ":", block,
               { "elif", expression, ":", block }, [ "else", ":", block ] ;
while        = "while", expression, ":", block ;
for          = "for", identifier, "in", expression, ":", block ;
match        = "match", expression, ":", NEWLINE, INDENT,
               { ( "case", pattern | "else" ), ":", block }, DEDENT ;
assertion     = "assert", expression, [ ",", expression ], line-end ;
expression   = literal | path | asset | array | unary | binary | call | field | index | grouped ;
array        = "[", [ expression, { ",", expression } ], "]" ;
path         = identifier, { ".", identifier } ;
line-end     = [ comment ], NEWLINE ;
```

## Diagnostics and formatting

Diagnostics carry stable codes, exact half-open UTF-8 byte spans, primary and secondary labels, and
notes. `M011xxx` identifies lexical errors, `M012xxx` parsing errors, and `M013xxx` resolution/type
errors. Tooling consumes the structured representation rather than scraping prose.

`mod01 fmt` writes canonical two-space indentation and token spacing. `mod01 fmt --check` reports
drift without writing. The formatter refuses invalid syntax so it cannot silently reinterpret a
broken module.
