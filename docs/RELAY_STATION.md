# Relay Station

A small playable game, not a scene engine or a test-only cartridge fixture. Its
one editable [workcart](../cartridges/relay-station/cart.m01w) embeds three current
members using only public MODL APIs:

| Alias   | Role                                                         | Persistent save ID    | Update rate                       |
| ------- | ------------------------------------------------------------ | --------------------- | --------------------------------- |
| `main`  | Station, docking and returned-clear counter                  | `relay-station`       | 60 Hz                             |
| `vault` | Link three relays and exit right                             | `relay-station-vault` | 30 Hz                             |
| `night` | Alternate indexed sprites and oscillator patch; not executed | `relay-station-night` | Irrelevant donor metadata (60 Hz) |

Run `target/debug/mod01 run cartridges/relay-station`, or open the workcart in
native Studio. Arrows move. At the station, walk to the door and press Z, press
Enter from anywhere, or choose DOCK AT VAULT in the Escape menu. In the vault,
press Z near the three highlighted relays from left to right, then at the right
exit. X toggles the day/night bank at any point. Escape offers a return without
winning and the same bank action. Simultaneous X/Z gives the bank request priority
so ordinary input cannot queue two transitions in one frame.

Docking passes the explicit `"dock"` parameter. A clear returns `"cleared"`; an
early return passes `"returned"`. Full loads start fresh program/RAM/task/input
context, while committed visit/clear values belong to each member's distinct ID.
Both members use the same save key names intentionally; owner assertions detect
incorrect identity routing. Returning after a clear shows the station's second
visit and first clear, with the vault's separate first visit/clear retained.

The day/night swap changes actual sprite bytes and triangle/pulse sound data.
The vault stays active at 30 Hz; link progress, player position, task pulses,
RNG, RAM, menu and local time survive. The HUD exposes ticks, pulses and RNG.
Reloading `"vault"` restores its own bank. Interactions sample every display
frame, including between 30 Hz updates. The data-only donor contains a deliberate
startup assertion, so an accidental full load/boot is a visible failure rather
than a plausible-looking bank change.

## Reproduce a complete run

The [110-frame route](../tests/replays/relay-station.json) includes held controls
on all four ports and both pointer buttons across docking, two data-only reloads,
all three links, the clear/return transfer, and held controls across returning.
It is an ordinary current `.m01rec` JSON trace; its `.json` extension is only for
source review.

```sh
target/debug/mod01 check cartridges/relay-station
target/debug/mod01 run cartridges/relay-station --headless --frames 110 --input tests/replays/relay-station.json
target/debug/mod01-studio --verify cartridges/relay-station --frames 110 --input tests/replays/relay-station.json
target/debug/mod01 pack cartridges/relay-station --output /tmp/relay-station.m01c
target/debug/mod01 export png cartridges/relay-station --output /tmp/relay-station.m01c.png
node scripts/verify-relay-station.mjs
```

The verification script compares all 110 frames' routing, work, indexed pixels,
interleaved PCM and audio commands across CLI/native source, raw, PNG and an
actual exported executable launched outside the checkout with empty PATH. Final
state/checkpoint and every member's save must also match. It uses a private
temporary directory and does not modify the maintained source, replay or saves.

The normal native export command ships the entire game, including the night
bank. Studio's member picker edits each embedded `src/main.modl` independently;
Run always starts the station. Inspect CLI `graph` for three separate compiler
models, not a merged hand-authored scene graph. `info --static` checks all members
without Node; normal `info` only profiles the actual initial input-free timeline
and correctly reports the other members as unobserved.
