# Cartridge pause actions

The shared compiler/core supports a bounded typed menu facility:

```modl
state shake: Bool = true

on start:
  menu_item(1, "SHAKE: ON")

on menu(action: Int):
  if action == 1:
    shake = not shake
    if shake:
      menu_item(1, "SHAKE: ON")
    else:
      menu_item(1, "SHAKE: OFF")
```

`menu_item(slot: Int, label: Text)` adds/replaces one of five ordered slots,
numbered 1–5. Labels are 1–24 printable ASCII characters. An empty label removes
that slot. Invalid arguments fault with `M019016` without modifying registrations.
The runtime charges `1 + label.length` units before mutation, in addition to
ordinary compiler call charges. Registrations are cartridge-owned state, not
host preferences, and are reset with the machine and retained in snapshots.
They may change during start, menu, update or draw, never during raster (including
calls through helpers). At most one `on menu` callback is allowed and its one
parameter must be `Int`. A cartridge without that callback has a no-op handler.

Opening or navigating the host pause menu submits no VM frame: frame-derived
time, updates, tasks, synth and saves remain stopped. An already submitted frame
may finish before the host reaches a message boundary. Selecting a cartridge
action closes the menu and schedules exactly one explicit action on the next
display frame. The native host supplies neutral controls for that action frame
so the selection press cannot also activate gameplay. This is not execution on
the UI thread and not an unrecorded host-side callback.

The shared scheduler invokes `on menu(action)` before the ordinary update/draw
sequence, on the same display frame/work budget. It runs even on odd 30 Hz frames
where `on update` is skipped; it does not increment the update counter itself.
The system phase register reports 6 during the menu callback. Starting tasks from
the handler uses the existing task scheduler; no extra task tick is inserted.
Selecting a slot that is no longer registered faults with `M019016` rather than
silently invoking a different action. Handlers can remove or rename their own
items; labels returned to the host reflect the actual runtime state.

The input frame and public replay record carry an optional `menuAction` integer
1–5. Absence means no activation; null, strings and out-of-range values are not
valid. Existing controls and pointer fields retain their normal meaning. Core
hosts may explicitly script an action together with other valid input, but native
menu selection itself uses neutral controls. Replay records the action on its
completed frame; menu navigation and wall-clock pause duration are not machine
inputs. A debug build steps the handler's real source statements and freezes that
frame's chosen action across pauses. Restoring a completed snapshot restores
registrations, inputs and counters and discards unfinished handler continuations.

The native common menu UI is a separate implementation slice in the acceptance
ledger; the core API alone does not mark that requirement complete.

This is MODL's typed design, not Lua callback compatibility. PICO-8's
[official custom-menu API](https://www.lexaloffle.com/dl/docs/pico-8_manual.html)
informed the bounded slots; its callback-return/left-right interaction conventions
are not part of this API.
