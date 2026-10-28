Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

## Beta.4.1 release integration — exact-image flashing prepared

The official beta.4 release remains pinned for read-only recognition. The separately published [`v0.6.0-beta.4.1`](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.4.1) catalog has SHA256 `5ec2dbddfb2d66d2afa13a408f0365d2474d8f94866f700d65d124eeb13f05c1`; it authorizes flashing exact beta.4 images for ten profiles, keeps settings writes false, and does not claim hardware validation. The installer selects beta.4.1 by default and requires explicit beta prerelease opt-in; beta.3 remains an alternate. Beta.4.1 uses release defaults and the existing fresh backup, confirmation, one-attempt loader and full readback gates. Saved-settings writes and restore remain blocked. See [STATUS.md](STATUS.md) for build and test evidence.

Validate the isolated Windows CI software OpenGL fixture on both native architectures after independent review; keep renderer and packaging gates. Source and local test evidence: [CI-WINDOWS-GL.md](CI-WINDOWS-GL.md). No Windows fixture runtime pass is claimed.

Use beta.4.1 as the default installation target and retain beta.3 as an explicit alternate. Beta.4.1 flashing uses only its exact pinned catalog and image bindings; do not infer authorization from beta.4 profile flags. Keep hardware validation false and settings writes/restore blocked. The separate QS v1/v2 model does not grant write authority.

Offline project import undo preserves unrelated subsequent profile edits; nonregular imports are rejected. File reads and atomic writes run in the dialog worker; native dialog and slow-filesystem behavior still need platform checks.

This plan tracks the agreed [requirements](REQUIREMENTS.md) for `dvino/ak-configurator`; firmware remains in `dvino/ak820`. Source implementation and offline verification do not authorize live keyboard operations or establish physical behavior.

## Alpha.1 release candidate — 2026-10-27

Application source version: `0.1.0-alpha.1`. The prior `0.1.0-beta.2` was unpublished development naming; no application release/tag exists. Firmware target: AK820 `v0.6.0-beta.3`, tag object `4cabae9af9bb68492a8cf079c59f8b388c5cf788`, peeled commit `88d190abb156960915a0f1f8ac7f602bb401eab3`. The [public firmware release](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was published on 2026-10-27 at 06:50:35 UTC. Firmware CI run [40461867751](https://github.com/dvino/ak820/actions/runs/40461867751) passed the shared-host job and “Revalidate and assemble all ten profiles”; its profile qualification matrix and publishing jobs were skipped. A later independent check confirmed the GitHub API digests for the catalog, firmware bundle, five JSON assets and `SHA256SUMS`; all ten IHX/settings identities match reviewed candidates, map hashes match the release matrix, and approved release metadata pins all ten installable images to the exact installer release. Physical device behavior remains unqualified. Exact application evidence belongs in [REVIEW.md](REVIEW.md).

| Workstream | Current source and checks | Limits and remaining evidence |
| --- | --- | --- |
| Image contracts | Ten exact beta.3 profiles, exact image/settings validation, explicit transitions; beta.2 legacy and alpha3 compatibility remain separate | Positive packed-image/full-ROM fixture and physical USB/settings qualification |
| Offline editor | Profile-specific controls, Combo with CapsWord result, `LT(layer0..5, MC0..15)`, AutoShift QSID 4 in Main and Typing; v2 projects pin release/catalog/IHX | Beta.2 backups migrate only through explicit reviewed source-to-target rules; beta1 v1 projects require deliberate manual migration |
| P1 RGB | Direct palette getter, full-frame atomic RAM commit, explicit previous Direct restore and `direct-ram.json`; CAVA 1.0.0 and OpenRGB SDK 0/1 retained | Firmware logical success does not prove LED scan/output. Runtime capture, physical timing/resources and real desktop/hardware qualification |
| Capture/installer | Any attempted capture failure blocks even defaults/rollback until reread; fresh install-only/OEM owner without loaded project settings may skip unsupported settings capture but still requires full-ROM/safety checks | Physical loader/port/readback and settings-retention qualification |
| Localization and help | Entropy navigation retained; compact picker/Combo and footer checked in RU/EN; localized install status and Desktop/Changelog/Status/Review guides with package links | Manual native UI checks on target desktops |
| Delivery | Optimized Linux x86_64 binary, ten-profile self-check and local trial install/upgrade/removal with user-data preservation passed; six unsigned package recipes include beta.3 inputs | Signing/notarization, real desktop-panel checks and physical qualification |

The firmware release is published and its release inputs have passed the documented source-identity and digest checks. CI itself skipped publishing and the profile qualification matrix. Release-input verification does not qualify physical behavior; a catalog entry, linked CODE fingerprint, synthetic fixture or CI artifact cannot prove device behavior. Private OEM inputs remain local. See [STATUS.md](STATUS.md) and [RGB-PLAN.md](RGB-PLAN.md).

## Handoff fixes and MCP integration — in progress, 2026-10-27

The current work starts at `fb4fb7b50c89d8fb99ed5e5cdac34aa3459ac4d7` and incorporates the supplied MCP plan. [MCP.md](MCP.md) describes the optional implementation; [MCP_IMPLEMENTATION_PLAN.md](MCP_IMPLEMENTATION_PLAN.md) retains the full original plan. Firmware identity and application version remain unchanged.

| Work | Current implementation | Remaining acceptance |
| --- | --- | --- |
| Handoff UI/helper | Tray callback lock release, RGB tab preservation during temporary HID ownership, 100 ms layout sampling and 1 s active override renewal; indicator controls moved to application settings | Actual helper/LED latency and Windows reproduction on hardware |
| Auto Shift | Controls follow exact active QSIDs; beta.3 Main/Typing show timeout only, with runtime key-action explanation; generic flags use checked readback | Persistent or remote enable control requires a future firmware contract; unchanged beta.3 cannot provide it |
| Launch feedback | Supplied logo before full editor initialization, background cold discovery, phase timing and visible/hidden startup gates | Current Windows/macOS launch timing and graphics/loader delays |
| Tray on supported OS families | Windows/macOS native tray; Linux StatusNotifier + Show/Quit menu, visible fallback and host-loss restore | Real desktop-panel checks and full Wayland tray hiding/restoration |
| P0.1 | Optional pinned official SDK, headless stdio, six closed tools, public catalog/resources, development subprocess harness | Windows stdio/GUI qualification and third-party client interoperability |
| P0.2 | FD00-first exact CODE recognition, typed FD01, diagnostics-only owner, stop-on-fault, no setters | Complete positive packed-image fixture, live helper/lease/cancellation qualification |
| P0.3 | Strict synthetic FD01/error suite, operation state, bounded metadata trace and in-memory reports | Full bootstrap replay, broader acceptance suites and persistent report artifacts |
| Runtime gate | Fresh private offline settings; actual native renderer, navigation and tray visibility checks; six-platform executable-bound candidate reports required | Final CI passed all six packaged candidates; repeat for any subsequent source change or release |

P0 remains partial and default-off. Production integration of provisional QS v2, backup/migration and protected updates has not started. The user requires pre-release native execution on Windows and Linux at minimum; the candidate verifier now requires the configured native smoke evidence on all six supported targets. No alpha is qualified or published by this work.

## Delivery order

1. Keep exact image identity, source-aware capability checks and directed transitions fail-closed. Preserve verified beta.3 installer pins and the separately pinned beta.4.1 exact-image flashing catalog; keep hardware-validation claims separate from image-install authorization. Keep release-input identity, platform execution and physical qualification as separate evidence.
2. Review restore, reset, deletion and installation with safety snapshots, confirmation pins, ownership and one-attempt failure handling. A failed read, timeout, storage fault or mismatch stops without retry or automatic rollback.
3. Qualify installation and OEM recovery only with explicit authorization and exact hardware procedures. Keep source, host-test and physical evidence distinct.
4. Complete native OS helper, capture permission and GUI teardown/reopen qualification per platform; retain the completed source/host checks.
5. Execute and inspect all six native packages, runtime/install reports and update index on the final source. Keep platform execution and signing evidence separate.
6. Continue RGB resource/timing and physical qualification. Begin browser feasibility after a usable native milestone; browser flashing remains later work.

## Acceptance boundaries

Host tests establish parsers, bounds, ordering and failure handling. Synthetic fixtures establish only their model. CI establishes only the source and runner actually executed. The final six-platform packaged CI pass is recorded in REVIEW.md; no physical keyboard operation is claimed here. Historical beta.2 sections of the changelog describe the previous development baseline, not the current firmware contract. [STATUS.md](STATUS.md) records current limits; [REVIEW.md](REVIEW.md) records current evidence.
