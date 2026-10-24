Русский перевод: [Состояние проекта](ru/STATUS.md).

# Project status

Updated 2026-10-24. The repository starts from Entropy commit
`5b222b5d6349fb351f53b36c66169095b3039fc1`, with its Git history and licenses.
Requirements and the implementation plan were moved from the AK820 planning
branch and updated for a separate configurator repository.

## Source, builds and device state

The Rust application is imported upstream source. Its Cargo package, executable
name, UI branding and hardware behavior still belong to Entropy; they have not
been changed to implement AK820 support. This is not an AK820-qualified client.
No AK Configurator binary, installer or website was built, released or deployed
in this migration. No USB commands or device tests were run.

Entropy provides native configuration and some Wasm-specific code, but no
working browser WebHID path was established by the source study. Native
adaptation starts first, while the shared browser path is developed in parallel.
The chosen foundation does not establish browser flashing feasibility.

## Repository and branches

`main` holds the shared reviewed source. Native development uses `desktop-dev`,
native release preparation uses `desktop`, and browser development uses
`web-dev`. These branches now belong to `dvino/ak-configurator`, not `dvino/ak820`.
Shared changes must be merged between development branches and checked for both
targets. Firmware compatibility is tied to exact firmware and configurator
versions across the two repositories.

The imported workflows are Entropy's build infrastructure. They are not a
completed AK820 qualification or cross-repository release gate. The imported
release workflow is restricted to the upstream repository. No upstream tags
are copied as AK Configurator releases.

The remaining work and release checks are in the [plan](PLAN.md).
Documentation checks, independent review and migration closure are recorded
in [REVIEW.md](REVIEW.md).
