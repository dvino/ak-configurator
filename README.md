Русский перевод: [AK Configurator](docs/ru/README.md).

# AK Configurator

<img src="assets/ak-configurator-logo.png" alt="AK Configurator logo" width="240">

AK Configurator adapts [Entropy](https://github.com/ergohaven/entropy) for the wired AK820. The first public preview, `0.1.0-alpha.1`, targets Windows x86_64 and ARM64. Download the per-user GUI setup or portable ZIP from the [GitHub release](https://github.com/dvino/ak-configurator/releases/tag/v0.1.0-alpha.1). Non-Windows builds, if listed, are experimental and unqualified; native macOS acceptance is outside this alpha's scope. See [Status](docs/STATUS.md) for validation and known limits.

The app pins the published AK820 firmware `v0.6.0-beta.5` and its ten exact profiles: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts and Multios. The in-app firmware installation page defaults to Beta5 and requires explicit prerelease opt-in; Beta3 compatibility checks remain, but Beta3 cannot be installed. The Windows setup only installs the app. Behavior on a real keyboard has not been verified, and saved-settings writes remain unavailable.

The native application includes an optional read-only MCP diagnostic interface. It can inspect bounded metadata and prepare a local installation preview; it does not expose a settings writer, restore executor or firmware flash operation. MCP use is separately enabled by the user. See the [MCP guide](docs/MCP.md) for setup and access boundaries.

The application can display and edit offline profile examples without connecting a keyboard. This does not establish complete keyboard configuration or physical support; see [Status](docs/STATUS.md) for the checked scope.

## Documentation

- [Installation](docs/INSTALL.md): current Windows candidate status, local build and package checks.
- [Desktop guide](docs/DESKTOP.md): application controls and current platform/device limits.
- [MCP guide](docs/MCP.md): optional diagnostics, synthetic checks and access boundaries.
- [Current status](docs/STATUS.md) and [review record](docs/REVIEW.md): evidence and remaining gates.
- [Requirements](docs/REQUIREMENTS.md) and [plan](docs/PLAN.md): project scope and delivery gates.
- [Changelog](CHANGELOG.md): user-visible changes and limits.

Firmware sources and releases remain in [dvino/ak820](https://github.com/dvino/ak820). OEM ROM and recovery material stay local. This repository preserves Entropy history, copyright notices and the GPL-3.0-or-later license; see [LICENSE](LICENSE).
