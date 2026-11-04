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

Локальная сборка x86_64 со static CRT прошла, как и self-check, офлайн-проверка GUI и MCP только для метаданных. PE inspection обнаружил 23 уникальных импорта ОС и не обнаружил обычных импортов `VCRUNTIME140.dll` или UCRT. Это не доказывает доступность dynamic/transitive зависимостей, совместимость со всеми версиями Windows или запуск на чистом ПК. CI-сборка ARM64 со static CRT, подпись и package ещё ожидаются.

Исполняемый файл Cargo не является полным дистрибутивом. Упаковка также подготавливает нативный CAVA helper и закреплённые публичные файлы прошивки. Текущий процесс упаковки Windows описан в [desktop CI workflow](../../.github/workflows/desktop-dev.yml) и [скрипте сборки кандидата](../../scripts/build_windows_ci_candidate.py). Установка финального пакета и локальное доверие сертификату ещё не проверены; сертификат разработчика не является сертификатом публичного CA.

Унаследованный Entropy `Taskfile` и скрипты упаковки Linux ожидают бинарный файл `entropy`. Они сохранены как история upstream-проекта и не являются квалифицированными инструкциями сборки AK Configurator для Linux. Квалификация Linux ещё не завершена. Приёмка нативной macOS не входит в утверждённую область этой альфа-версии.

См. [Установку](INSTALL.md) для статуса кандидата и запуска приложения. Для сборки приложения операции с физической клавиатурой не нужны.
