# Runnable MODL examples

Every path below is an ordinary project accepted by `mod01 check`, `run`, `test`, `pack`, and
Studio import. They use only public MODL/1 and the current hardware interfaces.

| Topic                                                                                                        | Small runnable source          |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------ |
| pixel, primitives, input, animation, sound, save, packing                                                    | `cartridges/modl-tutorial`     |
| sprites, maps, raster, custom fonts, synth/tracker, four ports, tasks, saves, modules, tests, bus, profiling | `examples/api-tour`            |
| focused RAM/ROM/endian/raster/audio/task/port stress                                                         | `cartridges/hardware-gauntlet` |
| procedural graphics/audio size coding                                                                        | `cartridges/signal-4k`         |
| a complete small game loop                                                                                   | `examples/relay-catch`         |
| software 3D, affine textures, collision, map editing, audio and best-time saves                              | `cartridges/prism-corridor`    |

Start with `mod01 run examples/api-tour`, then inspect its `cart.m01w`, which embeds both source modules and
`tests/smoke.modl`. `mod01 test examples/api-tour` runs its pure assertion and deterministic
scripted-frame example. The `templates/` directory contains deliberately small blank, arcade,
platform, grid/roguelike, and four-player starting points; they are examples, not a hidden engine.
