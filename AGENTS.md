Русский перевод: [Инструкции](docs/ru/AGENTS.md).

# Working on AK Configurator

@/home/danil/.codex/RTK.md

AK Configurator is based on Entropy. Keep upstream history, copyright notices
and third-party licenses. Firmware remains in `dvino/ak820`; do not copy its
whole tree into this repository.

- Read [requirements](docs/REQUIREMENTS.md), [plan](docs/PLAN.md) and
  [status](docs/STATUS.md) before implementation. Native adaptation starts
  first; the browser/Wasm stage starts after a reasonably usable desktop
  milestone. Keep protocol code reusable without adding a web implementation yet.
- Use `desktop-dev` for native work, `web-dev` for browser work and `desktop`
  for native release preparation. `main` is the common reviewed source.
  Merge shared changes instead of maintaining separate copies.
- Update documentation with code. Use plain impersonal wording and matching
  English/Russian documents. README introduces the project; installation
  actions belong in INSTALL, results and limitations in STATUS or reports.
- Distinguish source inspection, builds, host/CPU models, protocol tests and
  physical device behavior. Record skipped checks and do not invent a pass.
- Obtain independent review before commit or push. Record findings, closure
  and verification results. An author cannot approve the same changes.
- Offline development is allowed. Do not send device writes, reset, loader
  transitions or flash commands without explicit authorization for that
  operation. Launching inherited UI is not assumed to be read-only.
- Default test commands must be offline or read-only. Device tests must not
  require physical key presses, knob rotation, reconnect or manual unlock
  under the current automatic-test scope. A timeout, storage fault or bad
  readback stops the operation without blind retry, reset or restore.
- OEM ROM and recovery data stay local. No telemetry, typing analytics or
  automatic error uploads. Cloud features are optional and opt-in; they
  never include OEM data.
- Coordinate firmware, configurator and help release checks by exact version
  and image identity across repositories. CI without hardware does not prove
  flashing, retention or physical input. Each configurator release includes
  Linux and Windows x86_64 and arm64 builds, macOS x86_64 and arm64 builds,
  SHA256 files and matching RU/EN installation instructions. Do not add legacy
  32-bit targets.
- Follow [agent routing](docs/AGENT-ROUTING.md) for delegated work and review.
  Keep the [plan](docs/PLAN.md) current as work closes or new gaps are found.

Imported workflows, package names and UI branding are not proof of AK820
support. Keep their actual state clear until adaptation is implemented.

## Code, documentation and evidence

Adapted from firmware `AGENTS.md` at `bef20f89d3da01d770767919bc7c8fc3c3a1a808`.
Firmware stack and interrupt proof stays in the firmware repository; application
changes instead check parsers, bounds, operation ordering and supported targets.

- Update matching EN/RU documentation in the same work package as changes to
  behavior, formats, commands, versions, APIs or limits. Check agreement during
  independent review. Keep text plain, concise and impersonal; apply the
  write-like-me skill for substantial documentation without claiming unavailable
  style retrieval.
- README introduces the application. INSTALL explains download, checksum,
  extraction and launch. STATUS and technical reports record verification.
  Release notes describe user-visible changes and relevant limits.
- Check hashes, sizes, versions and commands against actual inputs and outputs.
  Regenerate affected reports. Replace obsolete current-state claims; retain
  their tracked history in Git and preserve unique uncommitted material before
  removing it.
- Prepare all firmware inputs and exact confirmation contents before erase.
  The intended in-app update requests a fresh three-second Esc + left Control
  hold, enters the loader, flashes once and verifies. No typed confirmation,
  separate Vial application or manual Bootloader step replaces that flow.
  Implementing its offline model does not authorize running it on hardware.
- Never promote catalog membership, a linked CODE fingerprint, a synthetic
  fixture or a CI binary to verified packed-image or device acceptance. Preserve
  exact image policies, explicit transition pairs and blocked unknown identity.
- Review code, documentation and test evidence together before commit or push.
  Record findings and closure; substantive fixes need retesting and a fresh
  independent review. Do not run builds sharing output paths concurrently.
