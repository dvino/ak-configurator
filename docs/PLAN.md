Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

This plan turns the agreed [requirements](REQUIREMENTS.md) into reviewable work for `dvino/ak-configurator`. AK820 firmware remains in `dvino/ak820`. It does not authorize live keyboard operations. Stage completion requires evidence and does not prove physical behavior.

## Active release preparation — 2026-10-26

Baseline: source commit `0642502`. The current source targets AK820 v0.6.0-beta.2, published 2026-10-26T04:52:40Z from peeled firmware commit `2990ef877c5c6c54013b8821450c5a91878b8c3a` (annotated tag object `2e0b28b384872c482e0e358a4b353f00ddd976a7`). Ten profiles are supported by exact packed CODE/settings identities, schema03 contracts, and explicit directional transition pairs. Application version remains `0.1.0-beta.2`; this work does not complete or publish a release.

| Workstream | Current state | Remaining evidence or work |
| --- | --- | --- |
| Profile contracts and live settings loader | Implemented for ten exact beta2 images; alpha3 retained as a separate exact path | Positive released packed-image/full-ROM fixture and physical connection qualification remain unavailable |
| Offline projects and typed modules | Project v2 pins target release/catalog/IHX; schema03 typed editors and all ten profile examples | Beta1 v1 requires manual migration; project and `.entlayout` files remain distinct from complete device backups |
| RGB | Built-in effects per profile; Direct palette editor and additive scene editor/compositor/HID playback implemented | 35 focused RGB host tests pass; Linux demo preview and exact scene JSON save/reload round trip cover 82 colors and two layers. Hardware timing, resource and platform qualification remain open; no persistent frame writes, palette getter or atomic frame commit |
| Backup and restore | Read-only beta2 capture through one HID session; complete NVM capture with supported-getter reconciliation; repeated NVM, manifest and status checks; private staged Linux save with reread and parent sync; bounded, fully validated history with pinned reopen; immutable restore preview | No live restore executor or safety-backup flow; Linux-only backup file I/O; no positive released full-ROM fixture or hardware qualification |
| Install and OEM recovery | Offline packer and install models | Installer UI/executor; full-image source and physical qualification; OEM-private helper bytes unavailable and excluded |
| Delivery | Versioned six-target candidate ZIP/provenance/SHA256 pipeline implemented; local/current-platform evidence is recorded in [REVIEW.md](REVIEW.md) | Six-target execution, installers, signing, and per-target runtime/function qualification |

Do not reduce release scope to offline-only: continue the complete beta2 release preparation while keeping hardware-dependent acceptance gates explicit. The missing OEM erase-helper prevents construction of the positive released packed-image/full-ROM fixture from public Intel HEX alone. It is not a reason to introduce private OEM data. The independent architecture review for this beta2 work is complete; no architect handoff is a current blocker.

Current build/test/review details are maintained in [REVIEW.md](REVIEW.md), and current support boundaries in [STATUS.md](STATUS.md). The separate [RGB plan](RGB-PLAN.md) remains in force. Browser implementation follows a reasonably usable native release; full requirements remain applicable.

## Delivery order

1. Maintain the exact firmware/image contract and fail-closed settings loader for every supported profile. Any new profile or transition requires exact packed identity and explicit directed evidence.
2. Implement a guarded restore executor with a fresh physical hold, safety backup and verified stop behavior. Then qualify capture, restore and cross-image handling. Current capture is read-only; no restore setter is connected. Native filesystem portability and full release qualification remain open.
3. Complete installer and OEM workflows only when image preparation, ownership, cancellation boundary, one-attempt behavior, installed-image verification and settings outcome separation are evidenced. Public HEX or synthetic models do not replace unavailable OEM bytes or physical evidence.
4. Prepare all six platform packages with matching SHA256 manifests and EN/RU install guidance. Run and verify current-tree CI, then record platform runtime, HID, function, resource, signing and installer evidence separately. Prior CI runs are historical unless their source matches the current tree.
5. Continue RGB qualification from the implemented native scene editor/compositor and bounded HID playback. Focused source/host validation is complete; next gates are platform timing/resource checks and device/HID qualification when authorized and available. Direct palettes and scene frames remain volatile; firmware provides no verified palette getter, persistence path or atomic frame commit.
6. Begin browser feasibility after the native editor and package milestone is reasonably usable. WebHID and static-host success do not replace physical qualification.

## Acceptance boundaries

Host tests establish parsers, bounds, ordering and failure handling only. Synthetic fixtures establish only the modeled input they contain. CI establishes builds for the source and runner configuration actually used. Platform launch, physical USB, persistence, flashing, OEM ownership and recovery require separate evidence. A failed read, timeout, storage fault or bad readback stops the relevant operation without blind retry, reset or automatic restore. No current stage authorizes device setters, reset, loader transition or flash.

The detailed requirements remain the acceptance source for privacy, source handling, target support, release coordination and browser behavior. Historical alpha3 and beta1 milestones remain in Git and review records; their findings are not evidence for beta2.
