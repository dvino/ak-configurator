Русский перевод: [Установка и разработка](ru/INSTALL.md).

# Installation and development

AK Configurator has no published application package yet. Do not treat an
Entropy download as an AK820-qualified release. Current source and validation
limits are listed in [Status](STATUS.md).

## Build the imported source

Install the Rust toolchain and the platform dependencies described in
[BUILD.md](../BUILD.md). From the repository root:

```sh
cargo build --release
```

This compiles the imported Entropy application, still named `entropy`.
It does not establish AK820 support. This migration did not run the command.
Launching the application may discover devices; a build alone does not open USB.

## Future packages

For each released AK Configurator package, RU/EN instructions must cover the
matching OS and architecture, download, SHA256 verification, extraction or
installation, and launch. Windows/Linux x86_64 and macOS arm64/x86_64 are
targets. Exact filenames and commands are added only after real packages exist.

Existing firmware installation and recovery remain documented in
[AK820 installation](https://github.com/dvino/ak820/blob/dev/docs/INSTALL.md).
