Русский перевод: [Состояние проекта](ru/STATUS.md).

# Project status

Updated 2026-10-27. Application source version is `0.1.0-alpha.1`. The previous `0.1.0-beta.2` was an unpublished development version; GitHub has no application release or tag for it. The firmware target is AK820 `v0.6.0-beta.3`, tag object `4cabae9af9bb68492a8cf079c59f8b388c5cf788`, peeled commit `88d190abb156960915a0f1f8ac7f602bb401eab3`. The [public firmware release](https://github.com/dvino/ak820/releases/tag/v0.6.0-beta.3) was published on 2026-10-27 at 06:50:35 UTC. CI run [40461867751](https://github.com/dvino/ak820/actions/runs/40461867751) passed the shared-host job and “Revalidate and assemble all ten profiles”; its profile qualification matrix and publishing jobs were skipped. A later independent verification confirmed GitHub API SHA256 digests for the release catalog, firmware bundle, five JSON assets and `SHA256SUMS`; all ten published IHX/settings identities match reviewed candidates, and the ten published map hashes match the release matrix. All ten profiles are marked release-approved/installable in `approved-release.json`, which pins the exact hashes from `installer-release.json`. The published `independent-review.md` records reproduced CI/source-review evidence, not a new human review. This establishes release-input identity, not physical device behavior. See [REVIEW.md](REVIEW.md) for application source and test evidence.

## Current source scope

The current source supports ten exact beta.3 profile images: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. Exact packed image/settings identity and required settings remain prerequisites for editing. The beta.2 identities are kept on a distinct legacy path; beta.2 backups migrate only through an explicit reviewed source-to-target migration, never by automatic promotion. Alpha3 compatibility remains exact and separate. The v2 offline project format pins its target release, catalog and Intel HEX image; beta1 v1 projects require deliberate manual migration.

The editor exposes beta.3 profile capabilities, including Combo with two distinct ordinary-key triggers and CapsWord as a result, plus the supported `LT(layer0..5, MC0..15)` macro action. This combined layer-tap/macro action is available only on the physical matrix; other actions retain their own source restrictions. AutoShift QSID 4 is available in Main and Typing. Beta.2 Main does not gain it. CAVA 1.0.0 and OpenRGB SDK 0/1 integrations remain intact.

The beta.3 Direct palette path reads the current palette, commits a complete frame atomically in RAM, and can explicitly restore the previously read Direct palette. `direct-ram.json` provides a separate explicit save/restore sidecar. It is not durable keyboard settings and does not write NVM. The source control owner and RGB scheduler cap sends at ten frames/s. Playback starts explicitly; faults stop without blind retry, automatic abort or restore. Successful firmware commit and logical readback do not prove physical LED scan output or visual qualification. See [RGB-PLAN.md](RGB-PLAN.md).

## Linux candidate checks

The optimized Linux x86_64 binary passed the offline ten-profile `--self-check`. A local pre-commit trial package passed installation, replacement, installed self-check and removal while preserving separate user data. Native demo inspection covers RU/EN at 800×500, key-picker scrolling and LT assignment, Combo output selection and timing, and separation of footer status from key hints. These checks use no physical keyboard or system capture. Exact test and package evidence is recorded in [REVIEW.md](REVIEW.md).

## Backup capture and installation

Once a settings capture is attempted, any failure stops preparation, including when defaults or rollback were selected. The failed owner remains blocked from prepare/execute until a fresh reread. A fresh install-only/OEM owner without loaded project settings may skip unsupported settings capture, but still requires exact full-ROM and safety checks. The OEM no-serial warning is limited to the first OEM source path.

The ten beta.3 release profile sources and verified assets are available for offline preparation and explicit prerelease installation. Package scripts now include localized Desktop, Changelog, Status and Review guides alongside the existing install/RGB guides, licenses, and offline beta.3 assets. Six native platform candidates are configured and unsigned. They have not completed the final six-platform build/runtime/signing pass and have not been published. No physical flash, retention, OEM rollback, or LED behavior is claimed.

## Verification boundary and remaining work

Firmware CI passed the shared-host job and revalidated/assembled all ten profiles; it skipped the profile qualification matrix and publishing. The later public release and source-to-release identity checks are complete as described above. Application package configuration is not a completed release. Backup history limits, guarded restore/reset/deletion steps, and filesystem/platform constraints are detailed in [DESKTOP.md](DESKTOP.md). The complete six-platform native pass, installed execution on the other five targets, signing/notarization and device qualification remain open. The verified Linux trial is recorded separately above. Verified release inputs do not establish physical device behavior.

Historical beta.2 documentation and changelog entries describe the earlier development baseline; claims that Direct palette readback or atomic commit are absent apply only to that historical firmware. Beta.2 backup migration is supported only through an explicit reviewed source-to-target path. Historical CI runs, isolated adapter typechecks and earlier platform checks do not establish current-source qualification. Browser support remains a later stage. Source implementation, host tests, platform execution and physical behavior are tracked separately.

Windows local fix: HID now routes FD02 replies to OS indicator validation. The local build uses the supplied AK Configurator icon for the Windows executable, window and tray. The matching test passes; physical indicator and tray restore remain unverified.

On a Russian-locale Windows host, the full suite reported 1691 passes, 11 failures and 5 ignored. All 11 passed individually after test fixtures explicitly selected English where expected; the held-HID test also has a longer test-only deadline. At this review, a full rerun had not finished.

The OS helper cannot enter the background after a session or helper error. The error stays visible while the device is unverified. Targeted offline tests passed, and the rebuilt Windows x86_64 package was installed and self-checked. Physical behavior remains unverified.

The tray setting takes priority over the automatic helper handoff. The Windows close-event test passed; restoring a real window from the tray remains unverified.

The local Windows full-suite rerun was stopped after test code wrote to the real application configuration. Unit-test storage, configuration and cache paths now use a temporary root; unit tests cannot change autostart. Five storage tests and one startup/cache path test passed. Other filesystem paths, system APIs, network access and spawned production binaries still need isolation review; this does not qualify the full suite as safe to run on a user account.

Unit-test device discovery and the reviewed HID open paths now fail closed; the connection worker requires an explicit test device. Unit tests skip native tray creation. Focused Windows tests passed for these paths and the existing indicator and tray decisions; 42 connection and 15 scan tests also passed. A real tray restore and physical keyboard behavior remain unverified.
