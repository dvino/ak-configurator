Русский перевод: [Возможности и ограничения](ru/DESKTOP.md).

# Desktop scope and limits

Application source version: `0.1.0-alpha.1`. Firmware source target: AK820 `v0.6.0-beta.3`, tag object `4cabae9af9bb68492a8cf079c59f8b388c5cf788`, peeled commit `88d190abb156960915a0f1f8ac7f602bb401eab3`. Firmware CI run [40461867751](https://github.com/dvino/ak820/actions/runs/40461867751) passed the shared-host job and “Revalidate and assemble all ten profiles”; the profile qualification matrix and publishing jobs were skipped. The published beta.3 release inputs can be installed with explicit prerelease opt-in after exact identity checks. The [AK820 `v0.6.0-beta.3` release](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was published on 2026-10-27 at 06:50:35 UTC. The release catalog, firmware bundle, five JSON assets and `SHA256SUMS` have been downloaded and their SHA256 values match the GitHub API digests. All ten published IHX/settings identities match reviewed candidates, and all ten map hashes match the published release matrix. Approved release metadata pins every installable image to the exact installer release. This confirms release-input identity, not physical device behavior. See [STATUS.md](STATUS.md) and [REVIEW.md](REVIEW.md).

## Supported images and editing

The beta.3 source catalog supports ten exact packed images: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. Editing requires complete image/settings identity. Beta.2 is a separate legacy path. Backup migration is supported only through an explicit reviewed source-to-target path; the catalog never promotes a backup automatically. Alpha3 compatibility is also exact and separate. Unknown/custom or mismatched images are rejected. A profile name or linked CODE hash does not establish support.

The desktop UI integrates the AK820 workflows into Entropy navigation. Language and Help are available before connection and follow the saved language. Utilities open only on request. The demo project uses bundled examples and does not represent settings read from a keyboard. Project format v2 pins the target release, catalog and Intel HEX. Beta1 v1 projects require deliberate manual migration.

Profile-aware editors include keys, encoders, macros, Combo and typed settings. Combo uses two distinct ordinary-key triggers and supports CapsWord as its result. For `LT(layer0..5, MC0..15)`, select a physical key, then **Special → Macro → Hold layer / tap macro → L0–L5 → M0–M15**. This action is valid only on the physical matrix; it is unavailable inside a macro stream, in Combo and on an encoder. AutoShift QSID 4 is available in Main and Typing, not beta.2 Main. Dynamic Macro and runtime modes remain firmware features and are not emulated on the host.

## Startup and Auto Shift

A visible launch shows the application logo and preparation state before the full editor loads. Device discovery then runs in the background. Minimized Windows startup goes directly to its tray lifecycle without waiting for an invisible splash.

In beta.3 Main and Typing, the Auto Shift page exposes the supported timeout. This firmware does not offer an enable switch through its settings protocol. Assign AS_ON, AS_OFF or AS_TOGGLE to a supported physical key to control its runtime state. The configurator does not display inactive switches or imply that changing a stored flag enables this feature.

## RGB

RGB is available only where the exact image advertises the capability. The beta.3 Direct path reads the current palette, commits complete frames atomically in firmware RAM, and supports explicit restoration of the previously read Direct palette. `direct-ram.json` is an explicit sidecar for volatile palette save/restore; it is separate from settings backups and does not write NVM. Geometry is 82 LEDs over 83 physical keys; encoder press has no LED. HSV V is at most 160; reads are limited to seven LEDs and writes to eight per chunk.

Built-in effects, scenes, CAVA 1.0.0 music input and optional local OpenRGB SDK 0/1 input share source ownership and a latest-frame scheduler capped at ten frames/s. Starts are explicit. Errors, loss of ownership and mode/profile changes stop playback without blind retry, automatic abort or restore. A committed frame or palette read does not prove physical LED scan output, timing or visual quality. See [RGB-PLAN.md](RGB-PLAN.md).

## Capture, backup and installation

Once settings capture has been attempted, any capture failure stops preparation, including when defaults or rollback were selected; those choices do not bypass the failure. Prepare/execute stays blocked until a fresh reread. A fresh install-only/OEM owner without loaded project settings may skip unsupported settings capture, but still requires exact full-ROM and safety checks. The first OEM source path has a specific no-serial warning. Installation requires exact source checks, a fresh preview/read, safety data, explicit confirmation and one attempt. Errors stop without retry or automatic rollback. No physical flashing or retention qualification is claimed.

Ten beta.3 profile sources and the published release assets are available for offline preparation and explicitly opted-in installation after exact identity checks. The [AK820 `v0.6.0-beta.3` release](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was published on 2026-10-27 at 06:50:35 UTC. The release catalog, firmware bundle, five JSON assets and `SHA256SUMS` have been downloaded and their SHA256 values match the GitHub API digests. All ten published IHX/settings identities match reviewed candidates, and all ten map hashes match the published release matrix. Approved release metadata pins every installable image to the exact installer release. This confirms release-input identity, not physical device behavior. Release observations bind to the exact beta.3 tag and catalog. A beta.2 withdrawal observation is scoped to beta.2 and does not block beta.3. Private OEM ROM and recovery information stay local. Backup capture, restore, reset and recovery paths are distinct from offline project files and volatile RGB sidecars.


### Backup, restore, reset and deletion

For a verified supported image with no pending edits, **Backups → Read complete snapshot** captures live getters and saved NVM, retains inactive typed bytes, and repeats NVM, ROM-manifest and storage checks. A failed check or cancellation stops without retry. The snapshot remains in memory until **Save new backup** publishes it to a separate dated folder through private staging, rereads and validation, and no-overwrite publication.

The backup browser checks only immediate folders (up to 128 backups and 1024 root entries), fully validates entries, and pins the manifest digest when reopening one. Restore requires a fresh device snapshot matching the immutable preview, a verified safety backup, a new three-second Esc + left Control hold, and one checked attempt per whitelisted setter. Final ROM, getters, NVM and manifest are checked. Failure stops without retry or automatic rollback; the safety reference remains available for explicit undo/reopen. Read-only sessions refuse restore.

Reset is separate: read a complete snapshot, choose **Preview reset to release defaults…**, review changes, then confirm. It uses defaults for the exact image, a matching fresh capture, safety backup, fresh hold and the same one-attempt executor. Inactive, reserved and inaccessible bytes stay unchanged. **Delete this backup…** reopens and validates the archive against its pinned manifest digest before deleting only known files after a separate permanent-deletion confirmation; it does not recursively remove folders. A filesystem error can leave a partially deleted archive.

Backup file operations are implemented on Linux, macOS and Windows. Unix paths are directory anchored; macOS rejects parent ACLs. Windows uses local-drive handle-relative operations, rejects reparse points and hard links, applies private user/SYSTEM ACLs, syncs files and publishes without replacement. Windows does not promise Unix parent-directory fsync semantics; UNC/device paths are unsupported. A cooperative per-device guard is advisory and is not honored by external Vial. `.entlayout` is a partial editor snapshot, `.ak820project.json` an offline project, and RGB palettes/scenes or `direct-ram.json` are separate sidecars; none replaces a complete device backup.

## Application settings, tray and exit

The OS layout indicator is configured in **Application settings**. Its enabled state and EN/RU mappings are application preferences. The helper samples layout at a 100 ms target interval, polls in-flight completion at 20 ms and renews an unchanged active override at one second. The same policy applies while the window is hidden. Host scheduling, native layout queries and transport latency can add time; physical LED timing is unqualified.

With close-to-tray enabled, the normal window close action hides the window and preserves background operation. Explicit **Quit application**, Ctrl+Q (also Cmd+Q), or the tray's Quit action requests full exit and does not start another helper. Existing pending-write, unlock and installation guards still apply. There is no separate “Close interface” button.

Windows uses the system tray and macOS the menu bar. Linux uses the session bus StatusNotifier protocol and a Show/Quit menu. A compatible desktop tray host is required; otherwise the window stays accessible. Loss or replacement of the host requests restoration. A synthetic host validates the protocol in CI; it does not qualify a real panel or Wayland. Protocol reference: [Status Notifier Item](https://specifications.freedesktop.org/status-notifier-item/latest-single/).

Offline GUI launch with `--demo-ak820` or `--runtime-smoke` uses fresh temporary settings and blocks reviewed live device/system entry points. These flags cannot be combined with other application modes. The native smoke and optional read-only agent diagnostic entry point are described in [MCP.md](MCP.md); exact execution evidence is in [REVIEW.md](REVIEW.md).

## Platforms and qualification

Six native unsigned platform candidates are configured. Final native build/runtime, installed behavior, signing/notarization and hardware checks remain open. CI, source inspection and host tests do not establish USB behavior, physical input, settings persistence, firmware flashing, OEM rollback or LED output. See [INSTALL.md](INSTALL.md) for package use and [STATUS.md](STATUS.md) for current evidence.
