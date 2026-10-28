Русский перевод: [Инструкции](docs/ru/AGENTS.md).

# Working on AK Configurator

@/home/danil/.codex/RTK.md

## Project and branches

- Preserve Entropy/upstream history, copyright and third-party licenses.
  Firmware stays in `dvino/ak820`; do not copy its whole tree here.
- Before implementation, read [requirements](docs/REQUIREMENTS.md),
  [plan](docs/PLAN.md) and [status](docs/STATUS.md). Native adaptation comes first;
  browser/Wasm work follows a usable desktop milestone. Keep protocol code
  reusable without implementing the web client at this stage.
- Use `desktop-dev` for native work, `web-dev` for browser work, `desktop` for
  native release preparation and `main` for common reviewed source. Merge
  shared changes rather than maintaining copies. Keep PLAN current.
- Follow [agent routing](docs/AGENT-ROUTING.md). Check ownership and preserve
  concurrent work; do not run builds sharing output paths.
- Agents must not perform recurring checks of CI, releases or other awaited
  external state themselves; use a monitor and act on its notifications. Do not
  repeatedly poll that state in an agent turn. One-time checks needed for active
  work remain allowed.

## Documentation and review

- Update matching EN/RU documents with changes to behavior, formats, commands,
  versions, APIs and limits. Use plain, concise, impersonal wording and
  write-like-me for substantial text without claiming unavailable style retrieval.
- Always check documentation consistency before commit or push: compare current
  claims with source and verified evidence, match EN/RU counterparts, and mark
  historical results as such. Resolve contradictions across README, INSTALL,
  PLAN, STATUS, REVIEW and affected guides.
- README introduces the app; INSTALL covers download, checksums, extraction
  and launch; STATUS/technical reports record verification and limits. Release
  notes describe user-visible changes and relevant limits.
- Verify hashes, sizes, versions and commands against actual artifacts;
  regenerate affected reports. Replace obsolete claims, retain Git history and
  preserve unique uncommitted material before deletion.
- Before commit or push, obtain independent review of code, docs and evidence.
  Authors cannot approve their own changes. Record findings and closure;
  substantive corrections need retesting and fresh independent review.
- Before finishing a task, clean up temporary files, test processes and build
  outputs created for that task after checking their ownership and whether they
  are still needed. Preserve active builds, concurrent or uncommitted work,
  unique evidence and artifacts promised to the user; report anything retained.

## Protocol and device operations

- Check parsers, bounds, operation ordering and supported targets. Firmware
  owns stack/interrupt proof. Separate source, builds, host/CPU models, protocol
  tests and physical behavior; report skipped checks without inventing a pass.
- Local/offline development is authorized. Device writes, reset, loader
  transitions and flash require explicit authorization for that operation.
  Launching inherited UI is not assumed read-only.
- Default tests are offline or read-only. Current automatic device tests need
  no key presses, knob rotation, reconnect or manual unlock. Timeout, storage
  fault or bad readback stops the operation without blind retry, reset or restore.
- Prepare all firmware inputs and exact confirmation contents before erase.
  The in-app update flow requests a fresh three-second Esc + left Control hold,
  enters the loader, flashes once and verifies. Typed confirmation, separate
  Vial or manual Bootloader do not replace it. An offline model is not live
  authorization.
- Keep exact image policies, explicit transition pairs and blocked unknown
  identity. Catalog membership, linked CODE fingerprints, synthetic fixtures,
  package names, imported workflows/UI and CI binaries do not establish packed
  image identity or device support. State their actual status until adapted.

## Data and releases

- Keep OEM ROM/recovery data local. No telemetry, typing analytics or automatic
  error uploads. Cloud features are optional, opt-in and never include OEM data.
- Coordinate firmware, companion and help releases by exact version and image
  identity. CI without hardware does not prove flashing, retention or input.
- During beta, require firmware backward compatibility only with the immediately
  preceding build, keeping its CODE budget bounded. Match companion firmware
  support to that release policy; do not extend it back to alpha.
- Companion releases include Linux and Windows x86_64/arm64, macOS x86_64/arm64,
  SHA256 and matching EN/RU installation instructions. Do not add legacy 32-bit
  targets. These application targets do not change firmware installer policy.
