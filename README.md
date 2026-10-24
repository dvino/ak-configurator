Русский перевод: [AK Configurator](docs/ru/README.md).

# AK Configurator

AK Configurator adapts [Entropy](https://github.com/ergohaven/entropy) for the
supported keyboards of the [AK820 firmware project](https://github.com/dvino/ak820).
The native Rust/egui application is the starting point. The browser version
will share its editor and logic through WebAssembly.

The intended functions are key and encoder configuration, macros, Combo,
lighting, local backups, firmware updates and stock-firmware recovery.
Desktop adaptation starts first; browser development runs in parallel.

- [Requirements](docs/REQUIREMENTS.md): behavior and acceptance criteria.
- [Implementation plan](docs/PLAN.md): work order and completion checks.
- [Installation and development](docs/INSTALL.md).
- [Current status](docs/STATUS.md) and [review record](docs/REVIEW.md).

Firmware source and firmware releases remain in
[dvino/ak820](https://github.com/dvino/ak820). This repository contains the
configurator, not a copy of the firmware tree.

Entropy source history and copyright notices are preserved. The imported
project declares GPL-3.0-or-later; see [LICENSE](LICENSE). Third-party notices
continue to apply to their respective files.
