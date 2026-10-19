# Native MODL source tools

Open a workcart in Studio and choose CODE. Edits remain in the active workcart;
changing modules preserves unsaved text. Nothing is written until Save. The
editor uses the same Rust lexer, formatter, type checker and linker as the CLI.

| Action                                       | Shortcut / control                          |
| -------------------------------------------- | ------------------------------------------- |
| Literal find                                 | Ctrl/Cmd+F, FIND                            |
| Replace selected match / all matches         | Ctrl/Cmd+H, REPLACE                         |
| Next / previous match, wrapping              | F3 / Shift+F3                               |
| Close find or completion                     | Escape                                      |
| Go to line                                   | LINE number, GO                             |
| Function, task, callback or state navigation | FUNCTION / STATE NAVIGATION                 |
| Go to resolved definition, across modules    | F12, DEFINITION                             |
| Return to previous source location           | Alt+Left, BACK                              |
| Indent / outdent selected lines              | Tab / Shift+Tab                             |
| Toggle line comments                         | Ctrl/Cmd+/                                  |
| Duplicate line/block                         | Ctrl/Cmd+D                                  |
| Source undo / redo                           | Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y |
| Explicit completion                          | Ctrl+Space, COMPLETE                        |
| Canonical formatting                         | FORMAT                                      |
| Compiler-derived relationships               | GRAPH                                       |

Search is scoped to the active module, with optional case/whole-identifier
matching. It is literal, not regular-expression search. Empty queries do not
replace anything; replacements are bounded to 16 MiB and are undoable.
Character selections remain safe while editing invalid/non-ASCII text; the
compiler still requires ASCII MODL.
At small window sizes the tools section scrolls independently, leaving room for
source and diagnostics. Selecting a search field brings it into view.

Completion includes the full compiler-registered API, declared asset handles,
current module/routine candidates and qualified public imports. Select with the
mouse or arrows/Enter. It replaces a selected identifier or the prefix before
the cursor, leaving the caret after insertion. During incomplete source edits,
the last successful index can supply completion candidates; compiler diagnostics
and declaration navigation use the current source. Completion is not permission
to execute a type-invalid call.

Type and syntax diagnostics show the original module, line, column, stable code
and compiler explanation. Click one to select its source; hover for notes.
Project-link errors display the linker's error rather than inventing a location.
The bottom bar reports current line/column and source type-check status; packing
and asset-capacity checks remain separate operations.

## Code/state graph

The graph is a read-only view of linked, resolved compiler IR: callbacks,
functions/tasks, globals and referenced assets, with call/start/read/write/asset
edges. Select a node to highlight/filter relationships; select a source node or
edge to navigate. Invalid source produces no executable graph. It contains no
authored scenes, component engine or runtime event orchestrator.

`mod01 graph cart.m01w` exports the same original-source definitions,
references, diagnostics and graph as JSON, without running Node or the cartridge.
CRLF authoring offsets and imported-name rewriting map back to original source.

## Verification

`project_tooling.rs` tests cross-module definitions, typed/syntax diagnostics,
all relationship kinds and exact CRLF positions. Native source-editor tests
cover literal/word/case search, Unicode-safe selection, bounded replacements,
block transformations, API/import completion, and real egui text-widget keyboard
selection, outdent, undo/redo and completion acceptance.

Linux/Xvfb interaction QA at 1280×800 additionally exercised find/F12, comments,
undo, diagnostics and graph navigation, and inspected native screenshots. It is
not a browser rendering test or a claim of Windows/macOS execution. Final product
verification remains in [NATIVE_IMPLEMENTATION.md](NATIVE_IMPLEMENTATION.md).

Cursor state and undo integrate with egui's actual
[TextEditState](https://docs.rs/egui/0.37.0/egui/widgets/text_edit/struct.TextEditState.html),
not a parallel text model.
