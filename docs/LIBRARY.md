# Offline cartridge library

Native Studio starts in LIBRARY when no cartridge was supplied. The toolbar's
LIBRARY button returns there without replacing the current workspace. Browsing
pauses execution and audio; BACK restores the prior pause state. A frame already
in flight may finish. No network service, account, catalog or JavaScript UI is used.

ADD CARTRIDGES bookmarks existing `.m01w`, `.m01c` and `.m01c.png` files. A directory
containing `cart.m01w` can also be opened through PROJECT. Successfully opened
cartridges enter the library automatically, including source inspection. Bundled
games appear alongside these bookmarks and are deduplicated by canonical path.
Bundled source opens as an unsaved copy: SAVE asks for a destination instead of
overwriting the installed example. Packed/PNG source also needs SAVE AS.

ADD FOLDER scans only the explicitly chosen directory, in the background. It
does not follow nested symlinks or descend into the assets of a discovered
`cart.m01w` directory. Limits are eight directory levels, 4,096 visited/queued
filesystem entries and 1,024 discovered cartridges. Skips and truncated scans
are reported; an over-capacity registration fails without partially changing
preferences. Studio never automatically crawls a home directory or drive.

Search matches all whitespace-separated terms, case-insensitively, across title,
author, cartridge ID, path and labels. FAVOURITES filters the list; RECENT FIRST
orders it by successful open/launch, with a stable title/path tie-break. Missing
or malformed cartridges remain visible with a diagnostic and can be forgotten.
REFRESH / RELOAD rereads preferences and cartridge metadata after external edits.

Select a cartridge and choose PLAY or INSPECT SOURCE. Opening another cartridge
with unsaved source, manifest, sprite or structured/raw asset changes asks for
SAVE & OPEN, DISCARD & OPEN, or CANCEL. Failed opens, failed saves, cancelled save
dialogs and invalid manifest text do not discard the previous workspace. Merely
browsing never writes cartridge source or save memory.

## Controls and preferences

- Mouse/touch selects a row and operates the same buttons.
- Up/Down selects; Z or Enter plays; X inspects source; Y toggles favourite.
- Controller D-pad selects, A/Start plays, X inspects, Y toggles favourite, B goes
  back. Controller Select remains host-only. Opening gestures are not launches.
- Shortcuts do not act while editing search/labels, while unfocused, or beneath
  the unsaved-work confirmation. BACK/Escape returns to an existing workspace;
  without a workspace it leaves the library open.

Each bookmark has a favourite flag and up to eight labels, each 1–32 printable
characters. Enter labels separated by commas and click SAVE LABELS. Labels are
trimmed and deduplicated case-insensitively. Selecting another cartridge or
refreshing abandons unapplied label text, not cartridge edits. FORGET BOOKMARK
removes only that local entry/preferences: it never deletes cartridge files or
save data. Bundled entries cannot be forgotten, but their preferences can change.

The displayed preferences path is the platform's MOD-01 Native Studio application
data directory, `library.json`, separate from `saves/` and cartridge formats.
Metadata is limited to 1 MiB / 1,024 entries. Writes use an OS lock, stale-content
check and same-directory atomic replacement. A second Studio must reload after
another instance changes preferences. Corrupt, unknown-revision or oversized
metadata remains untouched and read-only until corrected and successfully
reloaded; there is no silent reset or migration. Read-only preferences do not
prevent playing/inspecting bundled or already known cartridges.

## Covers and responsiveness

The worker reads actual cartridge metadata/source without running MODL or opening
the runtime. Packed/PNG cartridges use the ordinary shared integrity/compiler
validation. One request is processed at a time; stale refresh results are ignored.
Only the selected cover occupies the image cache, not every cartridge's RGBA data.

The embedded label (or thumbnail if no label is declared) supplies cover art.
Previews accept PNG up to 1,024×1,024 pixels or bounded static SVG shapes/text,
rendered to at most 240×144. Art input is limited to 256 KiB; SVG parsing permits
4,096 nodes, 32 levels and 8,192 text characters. SVG uses only Studio's bundled
BigBlue font, with external/data image resolution disabled. Images, scripts,
filters, masks, patterns and reusable SVG subtrees are not previewed. Unsupported
art shows a warning/fallback; it never changes source or prevents launching.

Renderer configuration follows the primary
[usvg options](https://docs.rs/usvg/0.48.1/usvg/struct.Options.html) and
[image resolver](https://docs.rs/usvg/0.48.1/usvg/struct.ImageHrefResolver.html)
contracts. No filesystem or network resource resolver is enabled.
