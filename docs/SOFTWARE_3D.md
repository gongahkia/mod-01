# PRISM CORRIDOR: software-rendered 3D

```sh
target/debug/mod01-studio --play cartridges/prism-corridor
# Source and the actual map/texture assets:
target/debug/mod01-studio --studio cartridges/prism-corridor
```

Z (gamepad south/A) starts a round. Up/down walk forward/backward; left/right
turn. A on the keyboard (gamepad west/X) toggles the map. Recover the gold prism
at map cell (4,5), then reach the cyan exit at (6,1). The map shows the player
and facing direction. A completed round records its best frame count in the
ordinary cartridge save. Z starts another round. Collision includes a player
radius, so corners cannot be crossed diagonally.

The single workcart contains all source and assets. `layout` is an ordinary 8×8
Map referencing the `blueprint` TileSet: zero is floor, one is solid. Native map
edits affect the same data used by collision and ray queries. `brick` and
`floor_texture` are ordinary indexed Sprite assets. No hidden geometry, scene
format or renderer plugin is involved.

## Rendering

MODL computes a direction and perpendicular camera plane from deterministic
`sin`/`cos`. Forty bounded grid rays locate the next solid cell and its texture
coordinate; each projects to six adjacent columns. Projection clips the visible
wall endpoints before issuing `texture_line`, preserving the texture's vertical
offset. Directional palette remaps shade alternate wall faces. Floor scanlines
provide their own affine texture start/delta and repeat the floor Sprite. Sky
uses the primitive fill-pattern API. The minimap, HUD, collision, objective,
timer, sound and best-time save are all ordinary cartridge code.

The geometric background is explained in the original
[raycasting](https://lodev.org/cgtutor/raycasting.html) and
[floor-casting](https://lodev.org/cgtutor/raycasting2.html) tutorials. This game
implements those mathematical ideas in MODL with explicit ray limits, public
texture commands and no copied host renderer. The console sampler's own bounds,
precision, wrapping and work rules are specified in [API.md](API.md).

## Reproduce the complete route

```sh
target/debug/mod01 check cartridges/prism-corridor
target/debug/mod01 run cartridges/prism-corridor --headless --frames 416 \
  --input tests/replays/prism-corridor.json --output prism-run.json
pnpm test packages/runtime/src/software-3d.test.ts
```

The public input trace starts, turns south, walks the west passage, turns east
through the prism, then turns north to the exit. Tests inspect compiler-derived
state names, assert the actual completed round and save, replay snapshots and
compare native/shared-core checkpoints in both release and debug. They verify
real texture commands and synth cues, not a mocked scene renderer.

The recorded 416-frame CLI route completes without a fault and peaks at 36,553
work units on this compiler, below the 50,000 ceiling. This is evidence for this
specific route, not a bound for arbitrary authored map edits. Native screenshots
of title, walking and the map were inspected separately; display-free checks
do not establish physical controller or speaker quality.
