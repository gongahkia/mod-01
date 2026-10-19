# Tutorial: build a playable cartridge

Open `cartridges/modl-tutorial/cart.m01w` with native Studio for the playable
FIRST SIGNAL tour. Its source is editable and inspectable. The walkthrough below
builds a small catching game with the same public facilities. Commands assume
the README toolchain and a built native player.

## 1. Create the project

```sh
cargo run -p mod01-cli -- new relay-catch --title "RELAY CATCH"
```

Open `relay-catch/cart.m01w`. Replace the `source` value of its
`src/main.modl` module with the following (retain the surrounding TOML quotes):

```modl
// RELAY CATCH
// Made by @gongahkia

enum Mode:
  Title
  Play
  Win
  Lose

state mode: Mode = Mode.Title
state paddle_x: Int = 104
state signal_x: Int = 120
state signal_y: Int = 18
state catches: Int = 0

fn next_signal() -> Unit:
  signal_x = rng_int(12, 228)
  signal_y = 18

on start:
  next_signal()

on update:
  if mode == Mode.Title:
    if btnp(pad1, a):
      catches = 0
      mode = Mode.Play
      next_signal()
  elif mode == Mode.Play:
    if btn(pad1, left):
      paddle_x -= 3
    if btn(pad1, right):
      paddle_x += 3
    if paddle_x < 0:
      paddle_x = 0
    if paddle_x > 208:
      paddle_x = 208
    signal_y += 2
    if signal_y >= 126:
      if signal_x >= paddle_x and signal_x < paddle_x + 32:
        catches += 1
        if catches >= 8:
          mode = Mode.Win
        else:
          next_signal()
      else:
        mode = Mode.Lose
  elif btnp(pad1, a):
    mode = Mode.Title

on draw:
  clear(25)
  if mode == Mode.Title:
    print("RELAY CATCH", 87, 48, 15)
    print("ARROWS MOVE", 88, 70, 23)
    print("Z TO START", 91, 88, 11)
  elif mode == Mode.Play:
    circle_fill(signal_x, signal_y, 4, 15)
    rect_fill(paddle_x, 132, 32, 5, 23)
    rect(8, 8, catches * 24, 5, 11)
  elif mode == Mode.Win:
    print("SIGNAL LOCKED", 82, 62, 23)
    print("Z TO REPLAY", 88, 82, 7)
  else:
    print("SIGNAL MISSED", 82, 62, 11)
    print("Z TO RETRY", 91, 82, 7)
```

The same checked example is kept at `examples/relay-catch` in this repository.

MODL uses two-space indentation, explicit persistent-state types, inferred local types, and
half-open ranges. `rng_int` is deterministic and `btnp` is true only on the press transition.

## 2. Check, format, and run

```sh
cargo run -p mod01-cli -- check relay-catch
cargo run -p mod01-cli -- fmt --check relay-catch
cargo run -p mod01-cli -- run relay-catch
```

`run` launches the native game surface. Use `--headless --frames 120` for
a deterministic non-graphical trace. Open the same workcart in native Studio to
edit, run, debug or inspect its compiler representation.

## 3. Pack and export

```sh
cargo run -p mod01-cli -- pack relay-catch
cargo run -p mod01-cli -- export png relay-catch --output relay-catch.m01c.png
```

The packed cartridge preserves the exact workcart, original source/assets,
compiler identity and optional JS/source map. PNG contains those exact packed
bytes. Add assets as embedded JSON blocks in the workcart, reference them as typed
handles such as `#hero`, and see [ASSETS.md](ASSETS.md). Native game packaging is
documented in [NATIVE_GAMES.md](NATIVE_GAMES.md).
