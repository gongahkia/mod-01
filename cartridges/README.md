# Bundled cartridges

The three original MODL/1 games remain the primary play cartridges. Every source and asset
file is inspectable, all cartridges build through the public `mod01` command, and each is credited
to `@gongahkia`.

- `cinder-circuit`: camera-scrolling tile platformer across four 2,048-pixel relay circuits. Arrow
  keys move, Z jumps, and Z/Enter starts; its 30 Hz update rate makes a clean uninterrupted run about
  4 1/2 minutes before platforming mistakes or life resets.
- `ashvault`: deterministic turn-based fog-of-war roguelike. Arrow keys move and Z/Enter starts.
- `raster-rush`: scanline pseudo-3D racer. Arrows steer, Z accelerates, X brakes, and A boosts. At
  the title, Z selects solo, A two players, S three players, and Enter four players. Standard
  gamepads map to the four controller ports; the second keyboard port uses I/J/K/L and F/G/R/T.
- `mod01-service`: source-visible current-machine service cartridge. It runs live checks over
  the public bus/API and presents eight diagnostic pages; A/right and B/left change pages.
- `signal-4k`: complete 4K-class procedural audiovisual transmission. It drives work RAM, raster
  palette/scroll and production synth; A retransmits its signal.
- `pocket-relay`: complete 16K-class one-minute catching game with custom bitmap digits, synth
  feedback, deterministic RNG and isolated best-score save.
- `hardware-gauntlet`: compact public-API stress display for bus/ROM, endian access,
  framebuffers, raster, four ports, wavetable audio, custom font, task scheduling and work counters.
- `modl-tutorial`: source-visible FIRST SIGNAL lesson that walks through a pixel, drawing, input,
  animation, synth sound, an application save commit, and the final pack command in five-to-ten
  minutes. A advances and B returns to the prior page.
- `prism-corridor`: complete first-person maze with textured walls, perspective
  floor, collision, a collectible prism, exit, synth feedback and isolated best
  time save. Arrows walk/turn, Z starts/restarts, and A toggles the editable map.
  The DDA/projection are ordinary MODL using `texture_line`, not a native 3D engine.
  See [software 3D](../docs/SOFTWARE_3D.md) for the design and reproducible route.
- `relay-station`: a complete three-member local game. Arrows move; Z docks at
  the station door, links three vault relays in order, then exits right. Enter
  also docks from the station. X swaps the vault's day/night sprites and synth
  patch without restarting its 30 Hz program, task, RNG, RAM or clock. Escape
  exposes the cartridge's dock/return/bank actions. The station and vault keep
  distinct visit/clear saves; the night member supplies data and never boots.
  See [bundle gameplay](../docs/RELAY_STATION.md) for the recorded complete route.

Each directory contains its single editable `cart.m01w`. Packed byte sizes are
measured by `mod01 info`, not source length or compressed code alone.

```sh
cargo run -p mod01-cli -- check cartridges/cinder-circuit
cargo run -p mod01-cli -- run cartridges/cinder-circuit
cargo run -p mod01-cli -- export png cartridges/cinder-circuit --output cinder.m01c.png
```

Raw/PNG/native exports are generated artifacts and stay out of Git.
