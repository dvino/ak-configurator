Русский перевод: [План](ru/PLAN.md).

# AK Configurator implementation plan

This plan turns the agreed [requirements](REQUIREMENTS.md) into reviewable work for `dvino/ak-configurator`. AK820 firmware remains in `dvino/ak820`. It does not authorize live keyboard operations. Stage completion requires evidence and does not prove physical behavior.

## Active release preparation — 2026-10-26

Baseline: source commit `c207ad13f3529bebccdbb8df15a7c38cfd96c1eb`. The current source targets AK820 v0.6.0-beta.2, published 2026-10-26T04:52:40Z at `2e0b28b384872c482e0e358a4b353f00ddd976a7`. Ten profiles are supported by exact packed CODE/settings identities, schema03 contracts, and explicit directional transition pairs. Application version is `0.1.0-beta.2`.

| Workstream | Current state | Remaining evidence or work |
| --- | --- | --- |
| Profile contracts and live settings loader | Implemented for ten exact beta2 images; alpha3 retained as a separate exact path | Positive released packed-image/full-ROM fixture and physical connection qualification remain unavailable |
| Offline projects and typed modules | Project v2 pins target release/catalog/IHX; schema03 typed editors and all ten profile examples | Beta1 v1 requires manual migration; complete backup import/export/history remains open |
| RGB | Built-in effects per profile; Direct palette editor for Main/Game/Mmo/Creative | Palette is volatile RAM-only; device persistence/readback and physical behavior remain unqualified |
| Backup and restore | Catalog-bound models; private backup-directory I/O on Linux | Full capture/history, complete UI/executor and exact device ownership |
| Install and OEM recovery | Offline packer and install models | Installer UI/executor; full-image source and physical qualification; OEM-private helper bytes unavailable and excluded |
| Delivery | Versioned six-target candidate ZIP/provenance/SHA256 pipeline implemented; local/current-platform evidence is recorded in [REVIEW.md](REVIEW.md) | Six-target execution, installers, signing, and per-target runtime/function qualification |

Do not reduce release scope to offline-only: continue the complete beta2 release preparation while keeping hardware-dependent acceptance gates explicit. The missing OEM erase-helper prevents construction of the positive released packed-image/full-ROM fixture from public Intel HEX alone. It is not a reason to introduce private OEM data. The independent architecture review for this beta2 work is complete; no architect handoff is a current blocker.

Current build/test/review details are maintained in [REVIEW.md](REVIEW.md), and current support boundaries in [STATUS.md](STATUS.md). The separate [RGB plan](RGB-PLAN.md) remains in force. Browser implementation follows a reasonably usable native release; full requirements remain applicable.

## Delivery order

1. Maintain the exact firmware/image contract and fail-closed settings loader for every supported profile. Any new profile or transition requires exact packed identity and explicit directed evidence.
2. Complete and qualify full backup capture, history, migration and guarded restore. Preserve all durable/inactive settings; reject unsupported schemas and cross-image operations.
3. Complete installer and OEM workflows only when image preparation, ownership, cancellation boundary, one-attempt behavior, installed-image verification and settings outcome separation are evidenced. Public HEX or synthetic models do not replace unavailable OEM bytes or physical evidence.
4. Prepare all six platform packages with matching SHA256 manifests and EN/RU install guidance. Run and verify current-tree CI, then record platform runtime, HID, function, resource, signing and installer evidence separately. Prior CI runs are historical unless their source matches the current tree.
5. Continue the RGB plan phases with source-model tests first, then device/HID and persistence qualification when authorized and available. Direct palettes remain explicitly volatile unless firmware gains a verified getter and persistence path.
6. Begin browser feasibility after the native editor and package milestone is reasonably usable. WebHID and static-host success do not replace physical qualification.

## Acceptance boundaries

Host tests establish parsers, bounds, ordering and failure handling only. Synthetic fixtures establish only the modeled input they contain. CI establishes builds for the source and runner configuration actually used. Platform launch, physical USB, persistence, flashing, OEM ownership and recovery require separate evidence. A failed read, timeout, storage fault or bad readback stops the relevant operation without blind retry, reset or automatic restore. No current stage authorizes device setters, reset, loader transition or flash.

The detailed requirements remain the acceptance source for privacy, source handling, target support, release coordination and browser behavior. Historical alpha3 and beta1 milestones remain in Git and review records; their findings are not evidence for beta2.
