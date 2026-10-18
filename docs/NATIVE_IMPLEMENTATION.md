# Native-only implementation contract

This is the implementation and verification ledger for the user's completed
`mod01-pico8-2026-10-18-d9918f7` review and subsequent native-only clarification.
The review contains 21 decisions. Notes override the original proposal: use one
current machine/format, remove backward compatibility, remove the browser frontend
entirely, and ship desktop games instead of HTML or web ZIPs. The shared compiler
and console core remain authoritative for native Studio, CLI, and packaged games.

Checkboxes mean the complete behaviour has been demonstrated, not merely that an
API, UI label, or test file exists. Each implementation slice receives an atomic
Git commit. The final audit must link the evidence for every row below.

| Requirement                   | Acceptance evidence                                                                                                                                                                                         | Complete |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| Native-only product           | Browser app, Wasm bridge, PWA, workers, web sharing/export and browser-only build/test dependencies removed; native build/check/package gates pass.                                                         | [ ]      |
| One current format            | Studio/CLI use one canonical editable workcart and one current packed encoding; no old-format/profile migration branches; authored modules, auxiliary files, layered sprites, recipes and tests round-trip. | [ ]      |
| Current machine contract      | API, hardware, assets, manual, examples, limits and metadata agree on the current 240×144 / 256-colour / 1 MiB machine.                                                                                     | [ ]      |
| Math helpers                  | Typed abs/min/max/clamp/rounding/sqrt/trig/atan2 execute with specified deterministic results, faults and work costs in release/debug/headless/native.                                                      | [ ]      |
| Number formatting/conversion  | Explicit numeric formatting, padding and safe parsing work in HUDs and reject invalid inputs predictably.                                                                                                   | [ ]      |
| Text helpers                  | Bounded length/slice/character conversion/split operations have useful typed results and tested edge cases.                                                                                                 | [ ]      |
| Bit operations                | AND/OR/XOR/NOT/shifts use explicit integer-width/overflow rules with parser, compiler and execution coverage.                                                                                               | [ ]      |
| Immediate MODL playground     | Native scratchpad/REPL evaluates expressions and drawing through the actual compiler/core; ordinary cart source remains editable and recoverable.                                                           | [ ]      |
| Native source editor          | Find/replace, line/function navigation, block shortcuts, completion, declaration navigation and actionable diagnostics are usable in the native UI.                                                         | [ ]      |
| Native tracker                | Full eight-channel grid with keyboard note entry, pattern/sequence operations, audition and playback feedback preserves real assets.                                                                        | [ ]      |
| Native game export            | CLI and Studio package a cartridge with a standalone native player and runtime; launch, input, audio, saves, source inspection and platform packaging are verified.                                         | [ ]      |
| Sprite-region blit            | Source/destination rectangles, stretching, clipping/flips, deterministic sampling and work costs are tested in a real cartridge.                                                                            | [ ]      |
| Textured lines                | Bounded texture sampling with specified coordinates, wrapping/clipping and work costs supports running pseudo-3D demonstrations.                                                                            | [ ]      |
| Pattern fills                 | Configurable two-colour pattern/mask state has defined transparency/reset/snapshot behaviour and real drawing tests.                                                                                        | [ ]      |
| Transparency controls         | Explicit draw transparency mask defaults, state reset, palette interaction and replay/debug restoration are verified.                                                                                       | [ ]      |
| Native touch controller       | Multi-touch D-pad/actions drive the ordinary controller/replay model in the native player without stuck/released-button errors.                                                                             | [ ]      |
| Pause/system menu             | Common native pause UI and typed cartridge-defined actions have specified input/timing/callback/replay/debug semantics.                                                                                     | [ ]      |
| Multi-cartridge bundles       | Local packaged dependencies, cartridge transitions, asset reload, state transfer, save identity and validation are implemented and tested.                                                                  | [ ]      |
| Local cartridge library       | Persistent labels/search/favourites/controller navigation/launch/source inspection cover user and bundled cartridges without a service.                                                                     | [ ]      |
| Examples and games            | Documented, playable and tested 2D and software-rendered 3D games demonstrate the added APIs using public facilities.                                                                                       | [ ]      |
| Full verification and handoff | Formatting/lint/type/unit/integration/native/package checks pass; native screenshots and interaction QA inspected; worktree clean; every requirement has authoritative evidence.                            | [ ]      |

## Excluded

- Browser frontend and web distribution, including standalone HTML and web ZIP.
- Historical-format/profile compatibility and migration support.
- Raspberry Pi/ARM handheld packaging.
- Public online catalog, accounts, hosted submissions or a community backend.
- A general-purpose 3D scene engine: 3D examples use software rendering and the
  public console facilities.

Native touch targets touchscreen desktops. Editable source and compiled cartridge
are source/distribution representations of the same current contract, not two
historical formats. Dependencies determine implementation order; no user priority
choices remain outstanding.

## Progress

- Scope revalidated against the actual downloaded JSON; baseline is `d9918f7`.
- Working branch: `feat/native-consolidation`.
- Implementation and final verification remain open.
- Browser Studio/Wasm/PWA removal: `5e17574`; browser adapters, sharing/storage
  and worker protocol removal: `09090dd`. Web export removal is still pending the
  standalone native game packager.
- Numeric helpers: `docs/STANDARD_LIBRARY.md`, `math.test.ts`, the compiler's
  numeric type tests, and `stdlib-conformance.test.ts` demonstrate release/debug,
  native/headless execution, independent accuracy vectors, bounded costs and
  snapshot replay. The final workspace gate remains mandatory.
