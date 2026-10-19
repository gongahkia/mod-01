# Native MODL playground

Open TOOLS → MODL PLAYGROUND / REPL, even without an open cartridge. This is a
stateless scratchpad: every evaluation compiles ordinary MODL and boots a fresh
instance of the production console. It is not a second expression interpreter
and does not retain variables between evaluations.

Choose EXPRESSION for a single-line value such as `sqrt(81.0) + 1.0`, or
STATEMENTS for drawing, loops and local variables. EVALUATE or Ctrl/Cmd+Enter
while an input field is focused runs the input. DRAW DEMO supplies a small
drawing example; evaluate it to see the result. Use spaces for indentation.
Pasted callback bodies are dedented while preserving their relative indentation;
Windows and Unix line endings are accepted.

DECLARATIONS / IMPORTS accepts ordinary top-level definitions, functions, state
and imports. The scratch input becomes an `on start` body, so declaring another
`on start` is a compiler error. INSPECT WRAPPED MODL SOURCE shows the exact
temporary module and COPY SOURCE lets you reuse it in your project.

## Results and isolation

Expression results and statement locals come from the real debug machine at the
final source boundary. The preview runs one actual frame, including any update
or draw callbacks provided in declarations. Statement output also includes state
after that frame. The 240×144 indexed frame uses the console palette and nearest
sampling. Audio is synthesized by the core but the scratchpad preview is silent.
Compiler diagnostics, runtime faults and work usage are displayed. Runtime fault
byte spans refer to the wrapped source available in the inspector.

The seed defaults to the same 604772761 used by native play and the CLI. Changing
it starts another fresh deterministic machine; an explicit zero is supported.
The normal 50,000-work-unit budget still applies. Source plus declarations is
bounded to 64 KiB, debug boot stepping to 65,536 boundaries and each runtime
command to five seconds. Displayed output is bounded to 8,192 Unicode characters.
Compilation runs off the UI thread. CANCEL discards the pending evaluation; a
cancelled compiler result cannot later start a console.

USE CURRENT ASSETS / MODULES makes a copy of the current project, including its
unsaved active MODL buffer, for asset references and imports. The scratch module
replaces the entrypoint only in that copy. Original entry callbacks are not run
unless explicitly supplied or imported according to ordinary MODL rules. The
active project, source file paths, dirty state and recovery files are untouched.
No cartridge save is loaded or persisted, including when save APIs are exercised.
The isolated runtime is discarded after completion or fault.

HISTORY retains the last 64 attempts in memory, including failed evaluations.
Selecting an entry restores its input, declarations, seed, context choice and
output, not a running machine or framebuffer. Evaluate again to recreate its
frame. The preview always labels the last successful frame; failures leave that
frame visible. History is not persisted when Studio closes.
