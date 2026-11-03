# Перенос и ревью issue — 01.11.2026

## Происхождение и полнота

Исходная сессия: `LOCAL_SOURCE/.pi/agent/sessions/SOURCE_SESSION/2026-10-29T07-22-15-030Z_01a18689-b876-70bc-a0e3-d30e4994a65f.jsonl` на исходном хосте (данные скрыты). Исходный worktree: `LOCAL_SOURCE/ChatGPT/ak-configurator-issue6-20261028`, detached на `936a989a8fd7d8e24cbd70ddba0e768e3107494b`. Удалённый worktree не изменялся.

Все зарегистрированные dirty-артефакты перенесены в `LOCAL_WORK/ak-configurator-beta5-review`, созданный из `origin/desktop-dev` на том же коммите:

- изменённые `docs/STATUS.md`, `docs/ru/STATUS.md`;
- новый `.github/scripts/test_beta5_catalog.py`;
- новые `assets/ak820/beta5-release-catalog.json`, `beta5-capability-catalog.json`, `beta5-firmware-matrix.json`, `beta5-module-descriptions.json`.

Других исходных файлов в сессии не было и они не копировались. Исходный JSONL и handoffs сохранены в `LOCAL_TMP/ak-configurator-review-mvxnftok/`; worktree и evidence сохраняются. Восстановленный Python-тест проходит (`Ran 1 test ... OK`). Он проверяет pinned digest, десять профилей, policy flags и междокументные identity, но не packed bytes и transition pairs.

## Результаты ревью перенесённой работы

- Staged beta.5 описывает superseded release `78dc69e6`, а не replacement source `c9e0220c25f664c4a470faa6700059f6aedf520c`. Это review inputs, не production pins.
- Перенос теста в `.github/scripts` закрыл проблему CI discovery. До stage 0 проверки остаются self-consistency, не проверкой final bundle/IHX/CODE bytes.
- Прежний отчёт: 1969 tests до финального исправления и 8 targeted; нет physical-device/full-ROM/platform GUI proof. Исторические тесты здесь не перезапускались.
- Packed CODE отличается от linked CODE. Release transitions не разрешают restore настроек. `settings_writes_authorized=false` обязателен.

## Сопоставление issue и кода

Полный GitHub inventory повторно проверен 01.11.2026: шесть issue #2–#7, все open; closed нет. Оригинальные body, URL и comments получены через `gh issue view --json number,title,body,comments,url` и сохранены в `LOCAL_TMP/ak-configurator-review-mvxnftok/issues-final-review/issue-{2..7}.json`. В #2 есть owner comment, отделяющий read failure от blocked/untested restore; у остальных comments нет. Матрица и этапы используют исходные acceptance criteria, не предположения по заголовкам.

| Issue | Текущее evidence | Работа и закрытие |
|---|---|---|
| #2 disconnected/lease | `src/ui/ak820_backup_inspection.rs` сводит capture errors к `Problem::Capture`; `src/ui/ak820_status.rs` включает native-operation busy; `src/ui/ak820_connection.rs` владеет reconciliation. | Сохранять domain causes, различать `OperationBusy`, `LeaseBusy`, `Disconnected`; stage 2 проверяет все состояния и stale generation. |
| #3 Windows RGB hang | Четыре setters в `src/ui/rgb_settings.rs` синхронно вызывают `hid.set_vialrgb_mode`; worker — `src/ui/vial_hid_task.rs`. | Одна submitted command и один unsent coalesced draft, bounded result/readback, no retry. Offline fix не закрывает Windows hardware acceptance. |
| #4 unlock | `src/vial/unlock.rs` `fail_vial_unlock_poll` пишет retrying и продолжает polling. | Timeout 15 секунд по virtual clock, terminal error/cancel, без automatic restart. |
| #5 layout flicker | Реальный seam — `Job::Sample`/`Job::Write`, completion polling и cached HID-loan display facts в `src/ui/ak820_os_indicator.rs`. Existing unchanged-sample, competing-work и cached-page tests дают частичную защиту; physical cause не установлен. | Stage 5 сохраняет защиту и добавляет 100 steady/100 alternating EN/RU frames с проверкой version/page/dialog/capture/connection stability. До закрытия требуется Windows read-only timing evidence. |
| #6 native navigation | Native pages и AppSettings service-navigation entry есть в `src/ui/ak820_status.rs`; entry и sizing label в `src/ui/layout_settings_dropdown.rs` — Wasm-only, native duplicate уже устранён. Byte-identical beta.4/4.1 не различаются по CODE; restore нужен первый шаг. `src/ui/app_settings.rs` использует localized keys. | Stage 6 сохраняет navigation/Wasm behavior, проверяет один native settings route, разделяет observed source identity/alias/provenance и target, поясняет inspection/blocked restore и RU editor/import/export scope. Новую архитектуру не создавать. |
| #7 OS detection и LED mapping | Реальный issue: EN detection рядом с EN=Off/RU=On mappings, не геометрия клавиатуры. В `src/ui/ak820_os_indicator.rs` уже частично пояснены mapping labels и разделены paused/detected states. Fixing history не установлен. | Stage 7 разделяет detection и saved LED behavior, поясняет modes EN/RU, сохраняет choices на pause. Тесты EN=Off/RU=On, detection changes, busy/unavailable; exact-firmware read-only mapping/LED observation обязателен. Geometry/encoder tests не относятся к issue. |

## Независимое ревью и проверки

Ревью ниже относится к прежней редакции и пропустило существенные acceptance gaps. Последующий completion audit обоснованно отклонил неверный scope #7 и неполное покрытие #5/#6. Разделы переписаны по оригинальным GitHub issue; последующее ревью и закрытие записаны ниже. SOL review `bfed151f-5431-4b47-8b09-b6b9d8014c13` вернул BLOCK с шестью planning defects: неверные команды тестов, MCP API/CLI, уже отсутствующий native duplicate, aliases beta.4/4.1, несогласованный R/L admission и устаревшая атрибуция #5 в этом отчёте. Документы исправлены по этим замечаниям; итоговое независимое закрытие записано ниже. Контракты «один этап — одна сессия» находятся в [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md).

Прежнее независимое focused review: `55d7f33b-27bf-4585-a7e6-ccdae0862f72`, 01.11.2026. Вердикт: **PASS — существенных оставшихся блокеров нет** в EN/RU отчётах переноса и исполнительных планах beta.5/MCP. Подтверждены issue-to-source mappings, удаление недоказанной атрибуции #7, реальные команды stage 0, совпадающие artifact-store contracts и safety gates. Reviewer не запускал builds, device/remote operations и не исследовал committed history; состав коммита основной агент отдельно проверил через `git show 936a989`.

Локальная проверка: `python3 .github/scripts/test_beta5_catalog.py` — один тест успешно; `git diff --check` и проверка локальных Markdown-ссылок успешны. Rust implementation и platform/device acceptance не заявляются. Commit и push не выполнялись. Временные recovery evidence и review worktree намеренно сохранены для передачи работы.

## Итоговая проверка планирования

- Broad SOL review `d06f4fc8-009d-4998-8e9e-a19d592a679a` подтвердил прежние исправления, кроме оставшегося MCP action mismatch, и нашёл нераспределённый SDK/Inspector interop, nonexistent unlock path и RU-команду выхода из worktree. Все четыре исправлены в EN/RU.
- Fresh focused SOL review `932c1b8c-78d8-4a68-959a-756830d93042` вернул **PASS** с file/line closure всех четырёх. Он использует source/architecture conclusions широкого ревью, не является новым полным code/hardware audit. Reports сохранены в `subagent-artifacts` родительской сессии под этими run IDs.
- Final local checks: `git diff --check` пройден; project-doc scan разрешил 210 ссылок в 40 файлах; 26 ordered session cards совпадают EN/RU; literal source paths/whitespace/invalid-command exclusions проверены. Воспроизводимый checker и evidence остаются в `LOCAL_TMP/ak-configurator-review-mvxnftok/`.
- `python3 .github/scripts/test_beta5_catalog.py` прошёл 1 test: self-consistency superseded recovered catalog, не final firmware admission/packing. Application/firmware builds и physical tests в рамках планирования не запускались.
- Повторная проверка GitHub на 2026-11-01 показывает только open #2–#7 с неизменными update timestamps. Issue не закрывались. Final pins, platform qualification, SDK/Inspector execution и отдельно разрешаемые physical observations — gates будущих сессий, не blockers завершения планирования.
- Recovery worktree и temporary evidence directory оставлены: там нужные uncommitted results/provenance. Commit/push, application/firmware implementation и device operations не выполнялись. Будущая работа начинается только по отдельно разрешённой карточке.

## Согласование beta.5

Candidate `c9e0220c25f664c4a470faa6700059f6aedf520c` завершил runs 41294887078/41294883546 с failure. Последний наблюдавшийся candidate на 2026-11-01 — `2ab372dab7f084b76b57aa98a9543c249a6d0c30`, runs 41297661542/41297648433 in progress. `gh release view beta.5 -R dvino/ak820` вернул `release not found`. Ранее run 41291741328 упал на static settings contract. Это исторические наблюдения, не финальные pins; перепроверить на stage 0. Финальные CI success и hashes не заявляются.

QS record v3 добавляет QSID23 offset 20, little-endian u16, 1–1000 ms, image default обычно 300. Читать v1/v2/v3 и моделировать lazy migration offline; saved-settings writes не включать. Разрешены только beta.4/4.1→beta.5 и beta.5→beta.5. Финальные assets и pack verification — обязательные gate исполнительного плана.
