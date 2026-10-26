# Desktop guide

## Supported beta2 images

The source supports the ten exact packed profile images published for AK820 v0.6.0-beta.2: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. Each profile has a schema03 settings contract (0x082003), an exact packed CODE/settings identity and explicit directional transitions. The active beta2 catalog is authoritative. The historical source registry is retained only for three schema02 backup identities; it provides no device aliases or beta1 write path. Unknown, custom, trial and mismatched images are rejected. Linked-window hashes do not substitute for packed CODE identity.

Connect the keyboard and allow the application to verify the complete image and load every required setting before editing. The editor supports six layers, 83 physical keys, and one encoder with both rotation directions and its ordinary matrix press. Per-profile assignment counts, QSIDs and source-specific keycode restrictions apply. Typed editors expose only their declared active records. Every typed module edit reads and validates full NVM before and after the change; unchanged defaults cause no setter. A failed write, timeout or readback mismatch stops the session until an explicit reread.

## Offline projects

Start `ak-configurator --demo-ak820` to work with bundled examples without USB access. Choose one of the ten beta2 profiles in the local-project window. These examples are not settings read from a keyboard. Project format v2 pins the target release, catalog and Intel HEX image. Version 1 beta1 projects are rejected and require deliberate manual migration. Saving/opening a project does not capture or restore keyboard settings or OEM firmware.

The schema03 panels provide eight Tap Dance records, eight Key Overrides records, or the active QMK setting IDs according to each profile contract. Unsupported or inactive storage is not exposed. Dynamic Macro and runtime modes remain firmware features; a key assignment does not emulate their physical behavior on the computer.

## RGB controls

The keyboard has 82 individually addressable LEDs and 83 physical keys; the encoder press has no LED. Brightness is limited to 160. Built-in effects are available according to each profile's contract. Direct palette editing is supported only for Main, Game, Mmo and Creative. It uses 82 local HSV colors in ten packets, with value capped at 160, sends at most nine LEDs per packet and requires an explicit RAM apply. The firmware API has no palette getter or persistence path; Direct palette edits are volatile and are not saved to NVM automatically. Saved mode 5 is translated to Direct for those four profiles and to Reactive for the other six, because the firmware uses profile-specific mode semantics. See the [RGB preset guide](RGB-PLAN.md).

## Backups and firmware operations

The local backup directory currently supports private file I/O only on Linux. Full backup capture/history, complete restoration UI/execution, installer UI/execution and exact physical-unit ownership are not complete. The `.entlayout` export is a partial editor snapshot, not a complete backup or recovery archive. Do not use it to restore a keyboard.

Image packing and installation are modeled offline. A positive released packed-image/full-ROM fixture is unavailable because the OEM erase-helper bytes are unavailable; public Intel HEX data alone cannot construct that image. OEM-private bytes are not included. No physical retention or HID timing qualification has been established. Firmware installation and OEM recovery remain gated. This guide describes source support and beta2 release preparation, not physical qualification.
