# Desktop guide

This alpha supports a wired AK820 with the verified full permanent firmware
v0.6.0-alpha.3 only. The application checks firmware compatibility when
connecting. Unknown, custom, trial or mismatched firmware is rejected.

## Offline beta profiles

Start `ak-configurator --demo-ak820` to work with bundled examples without USB
access. Choose a profile in the **Local example project** window.
The detailed catalog remains under **Layout → beta.1 firmware profiles**. Each profile keeps its edits while switching profiles in the same
session. These examples are not settings read from a keyboard. Use **Save project** in the local-project window to save the current profile
to an `.ak820project.json` file. **Open project** validates the complete file
before replacing its profile. Save each profile separately before closing
the application. **Undo project import** restores the example that was active
before the last successful import, including a same-profile replacement. Invalid module drafts must be corrected or discarded before
saving. These local project files preserve example edits; they cannot restore
keyboard settings or OEM firmware.

The key picker follows the selected profile and assignment source. The three
schema03 profiles have a separate offline panel: Tap Dance has eight rows,
Key Overrides has eight rows, and Settings exposes active QMK setting IDs only. Reopen a closed panel with
**Module settings** in the local-project window.
Invalid values leave the last valid settings unchanged. Other profiles do not
expose reserved extension storage. Dynamic Macros and runtime modes remain
volatile firmware features; changing a key assignment does not emulate their
physical behavior on the computer.

Selecting an offline profile neither changes a connected keyboard nor enables
beta firmware installation. The live connection still accepts the exact
alpha.3 image described below.

## Read before editing

Connect the keyboard and read its current settings. The editor supports six
layers, 83 keys, and one encoder with both rotation directions and its ordinary
matrix press. It also supports up to 16 macros with 8192 bytes total, ASCII
macro text and Vial macro codes 1–7; up to 8 two-key combos with a 40 ms term;
and RGB modes Off (0), Direct (1), Solid (2), Breathing (6), Cycle all (13),
Cycle left/right (14) and Reactive (31) for 82 LEDs, with brightness up to 160.
Direct mode can be selected, but this alpha has no per-LED palette editor. The
palette is kept in temporary device memory and cannot be read back.

After saving, the application checks the value read back from the keyboard. If
a write or its verification fails, stop editing and explicitly select **Read
settings again** before continuing.

## Macro unlock

To unlock macro editing, choose **Unlock** in the application, then hold Esc and
Left Ctrl together for three seconds until the unlock completes. This is an
explicit user action.

## Supported key actions

This firmware recognizes keycodes for Mod-Tap and Layer-Tap at 200 ms, simple
TT layer toggle, modifier-only One-Shot Modifier, tri-layer activation of layer
3 when layers 1 and 2 are active, volatile Default Layer, Caps Word, Key Repeat,
Alternate Key Repeat, NKRO and Space Cadet. Keycode availability describes
firmware behavior; only actions with a control in this editor can be configured
here.

Persistent default-layer actions, Tap Dance, mouse actions, arbitrary keycode
overrides and general QMK settings are outside this alpha's scope. An action
shown by the inherited Entropy editor is not necessarily supported by this
firmware.

## Backups and firmware operations

The existing `.entlayout` export is a partial editor snapshot. It is not a
complete settings backup, a recovery archive, or an OEM firmware backup. Import
for AK820 remains unavailable until compatibility and mandatory pre-backup
checks are implemented. Do not rely on `.entlayout` to recover the keyboard.

This alpha does not flash firmware, restore OEM firmware, or run an operating
system LED helper. Firmware installation and recovery are separate operations
and are not enabled by this configurator milestone. Browser support is planned
after the desktop path is reasonably working.
