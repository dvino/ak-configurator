# Сборка из исходников

English: [Build from source](../../BUILD.md).

## Сборка для Windows

Нужны нативная Windows MSVC toolchain для x86_64 или ARM64, Rust 1.94.1, инструменты C++ MSVC для выбранной архитектуры и Windows SDK. Для офлайн-проверок нужен Python 3.11 или новее. Из корня репозитория:

```powershell
cargo build --locked --release --features mcp
.\target\release\ak-configurator.exe --self-check
python scripts/test_mcp_production_metadata.py --binary .\target\release\ak-configurator.exe
```

Для параллельных сборок задайте каждой уникальный `CARGO_TARGET_DIR`, чтобы их выходные файлы не пересекались; в командах выше замените пути к исполняемому файлу на пути в этой директории. В [Cargo config](../../.cargo/config.toml) включён статический MSVC CRT для x64 и ARM64. При подготовке пакетов не переопределяйте настройку через `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS` или `CARGO_TARGET_*_RUSTFLAGS`.

Замороженная программная сборка `65e1a90041c434b98401203bb160e8986c4f0a46` прошла CI [41794559734](https://github.com/dvino/ak-configurator/actions/runs/41794559734): обе предварительные проверки подписи, все шесть нативных заданий, aggregate package contract вместе с EOL, четыре проверки PE parser и полный набор offline package-contract tests. Пакеты Windows x86_64 и ARM64 используют static CRT; у каждого приложения 23 OS imports, у helper — три. Прямых импортов VCRUNTIME/UCRT нет. Это не доказывает доступность dynamic/transitive зависимостей, совместимость со всеми версиями Windows или запуск на чистом ПК.

Исполняемый файл Cargo не является полным дистрибутивом. Упаковка также подготавливает нативный CAVA helper и закреплённые публичные файлы прошивки. Текущий процесс упаковки Windows описан в [desktop CI workflow](../../.github/workflows/desktop-dev.yml) и [скрипте сборки кандидата](../../scripts/build_windows_ci_candidate.py). Обе архитектуры упакованы, подписаны и независимо проверены; установка и MCP-проверка установленной версии прошли. Сертификат издателя локальный, не публичный CA. Root и TrustedPublisher текущего пользователя пока не доверены; статус подписи Windows — `UnknownError` для приложения, helper, PowerShell scripts и uninstaller. Ожидается ручное подтверждение ОС.

Унаследованный Entropy `Taskfile` и скрипты упаковки Linux ожидают бинарный файл `entropy`. Они сохранены как история upstream-проекта и не являются квалифицированными инструкциями сборки AK Configurator для Linux. Квалификация Linux ещё не завершена. Приёмка нативной macOS не входит в утверждённую область этой альфа-версии.

См. [Установку](INSTALL.md) для статуса кандидата и запуска приложения. Для сборки приложения операции с физической клавиатурой не нужны.
