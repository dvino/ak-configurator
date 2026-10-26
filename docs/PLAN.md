Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

This plan turns the agreed [requirements](REQUIREMENTS.md) into reviewable work for `dvino/ak-configurator`. Firmware remains in `dvino/ak820`. Source implementation and offline verification do not authorize live keyboard operations or establish physical behavior.

## Active release preparation — 2026-10-26

Working-tree baseline: `1c3dc18`. The target is AK820 v0.6.0-beta.2, published 2026-10-26T04:52:40Z from peeled firmware commit `2990ef877c5c6c54013b8821450c5a91878b8c3a` (annotated tag object `2e0b28b384872c482e0e358a4b353f00ddd976a7`). Application version remains `0.1.0-beta.2`. The current work implements native workflows; it does not complete or publish a qualified release. Exact build, test and independent-review evidence belongs in [REVIEW.md](REVIEW.md).

| Workstream | Implemented source | Remaining evidence or work |
| --- | --- | --- |
| Image contracts and settings | Ten exact beta2 profiles, schema03 contracts, directed settings transitions and full-image/settings loader; separate exact alpha3 path | Positive released packed-image/full-ROM fixture and physical USB/settings qualification |
| Offline editor | Project v2 pins release/catalog/IHX; ten examples and typed module editors | Deliberate manual migration of beta1 v1 projects; project and `.entlayout` files remain distinct from complete backups |
| RGB | Per-profile effects, Direct palettes, scene editor/compositor and bounded HID playback | Physical timing, resources and per-platform runtime; frames remain volatile without palette getter or atomic commit |
| Backup, restore and reset | Complete capture, private validated storage/history, guarded restore, release-default reset preview with verified safety backup and fresh hold, explicit digest-pinned backup deletion | Positive released full-ROM fixture and physical retention; current platform checks must be recorded separately from historical adapter typechecks |
| Firmware installation and OEM rollback | Bundled public beta2 IHX/map sources and default-settings packer; release admission and saved block-only withdrawal observations; immutable preview, exclusive native loader transport, fresh confirmation, durable safety snapshot and synced/reread trusted recovery registry, one-attempt flash and full-ROM verification; initial bundled OEM installation can use the exact fresh capture locally with explicit defaults and no preexisting file; provenance-bound local OEM rollback | Physical loader/port/unlock/readback qualification; Project updates/prepared payloads still require a supported personal OEM reference; invalid supplied inputs never fall back. OEM-private inputs stay local and are unavailable as public fixtures; broader release history/custom-build admission remains outside the pinned beta2 path |
| OS layout helper | Opt-in local EN/RU mappings to FD02 off/on/breathing, normal fallback, GUI worker and explicit `--ak820-helper` without initial GUI allocation; failure and ownership gates | Platform layout/HID, GUI teardown/reopen and resource qualification; unsupported compositor/layout handling on real systems |
| Delivery | Six-target portable archives: Linux `.tar.gz`, Windows `.zip`, macOS `.app.zip`, provenance/SHA256, localized guidance and unsigned DRAFT workflow/verifier | Current-tree execution for all six targets, platform launch/HID/functions/resources and signing; archive creation does not establish installed-system integration |

Continue the native release scope with explicit hardware gates. Public linked sources and independently derived defaults improve offline coverage but cannot construct an approved positive packed-image/full-ROM fixture without the OEM erase-helper bytes. Private OEM bytes are not added. The independent architecture stage is complete; a new architect handoff is not a standing blocker.

## Delivery order

1. Keep exact image identities, source-aware capability checks and explicit directed transitions fail-closed. Record any new image or transition independently.
2. Review and verify restore, reset, deletion and installer paths together with their safety snapshots, confirmation pins, ownership gates and one-attempt failure handling. A failed read, timeout, storage fault or mismatch stops without retry or automatic rollback. Preserve separate firmware/settings outcomes and saved safety references.
3. Qualify the implemented native installer on an explicitly authorized keyboard only when available. Check fresh hold, application/loader port binding, cancellation boundary, protected regions, full-ROM readback and OEM ownership separately. The local **Firmware files** checker remains an additional byte checker, not the installation action.
4. Verify the OS helper in GUI and explicit background modes. The helper must yield the owner to settings, backup, restore and installation; unknown layout releases indication and a fault cannot regrant the same endpoint merely after a delay. Qualify GUI teardown and reopening on each platform.
5. Execute and verify all six current-tree packages and matching SHA256/EN/RU instructions. Historical isolated Windows GNU/macOS arm64 typechecks do not establish current full-app builds or runtime. Keep portable packaging, OS integration and signing evidence separate.
6. Continue RGB timing/resource and physical qualification from the implemented native editor and bounded playback. Begin browser feasibility after a reasonably usable native editor/package milestone; browser flashing remains a later stage.

## Acceptance boundaries

Host tests establish parsers, bounds, ordering and failure handling. Synthetic fixtures establish only their model. CI establishes only the source and runner configuration actually executed. No hardware operation or six-target execution is claimed by this plan. Physical USB, input, persistence, flashing and recovery require separate evidence and explicit operation authorization. Historical alpha3/beta1 records remain in Git; they do not qualify beta2. [STATUS.md](STATUS.md) records current limits; [RGB-PLAN.md](RGB-PLAN.md) retains the detailed RGB phases.
