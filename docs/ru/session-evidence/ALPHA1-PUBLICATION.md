# Доказательства публикации alpha.1

**Опубликован:** 2026-11-04 12:16:16 UTC, prerelease `v0.1.0-alpha.1`, release 433846095.

Lightweight tag `v0.1.0-alpha.1` указывает на `71abd382cbcefa554c87639635536e5f526be226` в `main`. Исходники Rust-приложения, входы Cargo, build script, assets, определения установщика, i18n и vendored inputs совпадают с checkpoint `152d02d`. Позже менялись workflow-файлы, тест package workflow и документация. Source-to-binary attestation и эквивалентность бинарников исходникам не заявляются.

CI [41818771438](https://github.com/dvino/ak-configurator/actions/runs/41818771438) завершился успешно: шесть нативных заданий, две предварительные проверки подписи Windows и aggregate verification, задание 126549766798 (32 теста). На Windows x86_64 и ARM64 использовался Rust 1.94.1; на каждой цели прошло 1 978 нативных тестов, 10 пропущено, а в MCP прошло 34 теста, 6 пропущено. Windows GUI runtime smoke, сборка/проверка установщика и offline self-check прошли. Устаревшие Build и Nix задания пропущены из-за upstream guards. Точные задания и результаты приведены в JSON evidence.

Команды тестов Windows в workflow: `cargo test --locked --all-targets -- --test-threads=1` и `cargo test --locked --features mcp-mock --bin ak-configurator 'mcp::' -- --test-threads=1`; затем выполнены `python scripts/test_mcp_stdio.py --binary target/debug/ak-configurator.exe`, Windows GUI smoke harness и offline self-check. Aggregate verification запускала `python3 scripts/test_package_line_endings.py`, `python3 scripts/test_windows_runtime_dependencies.py` и `python3 scripts/test_ak820_release_workflow.py`; результаты — 1, 4 и 32 passed. Источник workflow закреплён в JSON CI. Это результаты CI, не локальный повтор и не проверка физического устройства.

В релизе 37 assets: 34 кандидата, `release-manifest.json`, `app-update-index.json` и `SHA256SUMS`. Размеры и SHA256 сервера совпали с локальной публикационной проверкой. Независимая проверка артефактов прошла, включая сравнение 34 файлов и проверку агрегатов manifest/index. Это не подтверждает сборку бинарных файлов из tagged source.

Публичные хеши приложения Windows: x86_64 `b1ceb499c244bd71ac30971f00827bf5d73ebda4ea4bdc6f5b29ca59590bdcee` (55 023 288 байт), ARM64 `502d468311c9b5125f3c4776a55142a2f1d20460849e1416c3c94aa47ac40124` (51 303 096 байт). У пользователя остаётся отдельно замороженная установленная сборка 65; публичный commit 71 не устанавливался. Root и TrustedPublisher текущего пользователя не доверены; Windows сообщает `UnknownError` для установленных подписей. Записи HID/устройства не было.

Это экспериментальный предварительный выпуск, разрешённый владельцем, а не принятие B12. Linux assets экспериментальны; интерактивная работа на ARM и использование на чистом ПК не квалифицированы; native macOS acceptance исключена. Независимые пункты аудита issues #2–#7 остаются открытыми до live acceptance. Не подтверждены поведение публичной сборки 71 после установки, физическая клавиатура, запуск на чистом ПК и source-to-binary provenance. Отдельная read-only проверка входов beta.5 прошивки включена как evidence.

## Сохранённые evidence

- `ALPHA1-PUBLICATION-RECEIPT.json`: идентичность релиза, имена файлов, размеры и публичные хеши.
- `ALPHA1-CI-41818771438.json`: очищенная сводка CI jobs и шагов.
- `ALPHA1-CI71-TEST-SUMMARY.json`: результаты Windows и aggregate, версия Rust.
- `ALPHA1-FIRMWARE-RECHECK.json`: проверка публичной идентичности входов beta.5; без устройства и сборки прошивки.
- `ALPHA1-INDEPENDENT-REVIEW.json` и `ALPHA1-BYTE-REVIEW.json`: очищенные результаты независимой проверки артефактов.

Эти отчёты не содержат частную резервную копию, ROM, пользовательские данные или локальные пути.
