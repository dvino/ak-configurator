Русский перевод: [Инструкции](docs/ru/AGENTS.md).

# Working on AK Configurator

AK Configurator is based on Entropy. Keep upstream history, copyright notices
and third-party licenses. Firmware remains in `dvino/ak820`; do not copy its
whole tree into this repository.

- Read [requirements](docs/REQUIREMENTS.md), [plan](docs/PLAN.md) and
  [status](docs/STATUS.md) before implementation. Native adaptation starts
  first and the browser/Wasm version develops in parallel from shared code.
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
  Windows/Linux x86_64 packages, macOS arm64/x86_64 builds, SHA256 files and
  matching RU/EN installation instructions.
- Use bounded tasks with explicit file ownership. Subagent communication is
  in English. Use a fresh GPT-6 Luna at medium reasoning for limited tasks;
  GPT-6.1 Sol at low reasoning for independent review or complex ABI/NVM work.

Imported workflows, package names and UI branding are not proof of AK820
support. Keep their actual state clear until adaptation is implemented.
