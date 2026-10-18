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
