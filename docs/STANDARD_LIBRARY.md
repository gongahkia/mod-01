# MODL standard library

These are typed console calls, not JavaScript host access. Studio, CLI, debugger,
and standalone players execute the same implementation and charge the same work.

## Numeric helpers

| Call                                                          | Result                                         | Additional work |
| ------------------------------------------------------------- | ---------------------------------------------- | --------------- |
| `abs(value)`                                                  | Magnitude; preserves `Int`/`Num`               | 4               |
| `min(a, b)`, `max(a, b)`                                      | `Int` if all inputs are `Int`, otherwise `Num` | 4               |
| `clamp(value, minimum, maximum)`                              | Same promotion; inclusive bounds               | 4               |
| `floor(value)`, `ceil(value)`, `round(value)`, `trunc(value)` | Safe `Int`                                     | 4               |
| `sqrt(value)`                                                 | Nonnegative square root, `Num`                 | 640             |
| `sin(radians)`, `cos(radians)`                                | `Num`                                          | 32              |
| `atan2(y, x)`                                                 | Angle in radians in `[-pi, pi]`, `Num`         | 96              |

`round` breaks ties toward positive infinity (`round(-1.5) == -1`); all helpers
normalize negative zero to zero. Reversed clamp bounds, non-finite arguments,
negative square roots, unsafe integer results, or trig angles outside
`[-1000000,1000000]` fault with `M019030` at the call site. `atan2(0,0)` is zero.
Arguments are evaluated normally before the listed fixed cost is charged; the
ordinary generated call/statement costs also apply.

Trig uses explicit range reduction and fixed arithmetic series, not platform
`libm` approximations. The error target for sin/cos is below `1e-10` throughout
the supported range and for atan2 below `1e-14`. Square root uses power-of-four
normalization and eight Newton iterations, including subnormal inputs. Results
are binary64 approximations, not symbolic real arithmetic. Replays reproduce the
same operations; they do not depend on wall-clock time or locale.

Executable assertions: [math.modl](../tests/conformance/math.modl). Tests compile
both release/debug source and execute it in the shared core, native child host,
and headless host; device output and snapshot replay are checked.

## Text, formatting and safe conversion

| Call                                                | Result                                                  |
| --------------------------------------------------- | ------------------------------------------------------- |
| `format_int(value: Int, base: Int, width: Int)`     | Signed `Text`, zero-padded after the sign               |
| `format_num(value: Num, decimals: Int, width: Int)` | Fixed decimal `Text`, left-padded with spaces           |
| `parse_int(text: Text, base: Int)`                  | `Option[Int]`                                           |
| `parse_num(text: Text)`                             | `Option[Num]`                                           |
| `text_length(text: Text)`                           | Unicode scalar count, `Int`                             |
| `text_slice(text: Text, start: Int, end: Int)`      | Half-open scalar slice, `Text`                          |
| `text_code(text: Text, index: Int)`                 | `Option[Int]` Unicode scalar value                      |
| `text_char(code: Int)`                              | `Option[Text]`; invalid scalars/surrogates yield `none` |
| `text_split(text: Text, separator: Text)`           | `List[Text, 256]`, preserving empty/trailing fields     |
| `text_pad(text: Text, width: Int, padding: Text)`   | Left-pad to scalar width; one-scalar padding            |
| `list_length(list)`                                 | Initialized array/list extent, `Int`                    |

Text helper inputs are bounded to 4,096 UTF-16 storage units before allocation.
Indexes, slices and lengths count Unicode scalars, not UTF-16 halves. Ordinary
`text[index]` uses the same scalar indexing but faults outside the string;
`text_code` returns `none` instead. Built-in glyphs are still the console font;
these helpers do not add Unicode glyphs to it. Generate non-ASCII characters with
`text_char` because MODL source itself is ASCII.

Integer bases are 2, 8, 10 and 16; width is 0–64, including the sign. Fixed decimal
precision is 0–12 and magnitude must be below `1e21`; binary64 rounding follows
ECMAScript's locale-independent fixed-decimal conversion. Width is a minimum,
never truncation. Parsing consumes the whole non-whitespace string (optional
sign, digits; decimal numbers may use a decimal point/exponent). No radix prefix,
numeric prefix, NaN or Infinity is accepted. Unsafe integers, invalid text or
non-finite parsed numbers yield `none`; parse inputs above 256 units yield `none`.

Slices must satisfy `0 <= start <= end <= text_length(text)`. Empty separators,
more than 256 split parts, invalid arguments or oversized helper input/padding
fault with `M019031`. Padding width is 0–4,096; padded storage must also fit 4,096
units. Helpers charge 4 plus all input text storage units before traversal;
padding additionally charges its output storage units before creating the output.
Split output is bounded by its input and the fixed part limit. Sparse list holes
are included in the extent but reading an uninitialized element still faults.

Executable HUD/conversion assertions: [text.modl](../tests/conformance/text.modl).

## Bit operations

`&`, `|`, `^`, `~`, `<<`, `>>`, `>>>` operate on `Int` words. Equivalent typed
calls are `bit_and(a,b)`, `bit_or(a,b)`, `bit_xor(a,b)`, `bit_not(a)`,
`bit_shl(a,count)`, `bit_shr(a,count)`, and `bit_ushr(a,count)`.

Inputs must be in `[-2147483648,4294967295]`: signed or unsigned spelling of one
32-bit word. Results use signed two's-complement `Int`, except `>>>`/`bit_ushr`
which return the unsigned value. Left shift wraps at 32 bits, right shift sign
extends, unsigned right shift zero-fills. Shift counts must be 0–31; counts are
**not** silently reduced modulo 32. Out-of-range words/counts fault `M019032`.
Each operation costs one additional work unit plus generated call costs.

Precedence, from tightest to loosest: unary, multiply/divide/remainder,
add/subtract, shifts, `&`, `^`, `|`, range, comparisons, equality, `and`, `or`.
Binary operators at one precedence associate left. In particular `1 << 2 + 1`
means `1 << (2 + 1)`, and `value & 255 == 1` means `(value & 255) == 1`.
Compile-time constant evaluation uses identical word/shift rules.

Executable operator/call assertions: [bits.modl](../tests/conformance/bits.modl).
