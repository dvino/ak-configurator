# C04 input guard — ограничение MCP JSON framing

English: [EN](../../session-evidence/C04-INPUT-GUARD.md).

## Состояние

**Только подготовительная transport guard; ограниченная проверка документации/evidence закрыта 2026-11-03 без оставшихся findings.** Этот отчёт фиксирует review guard до B11 acceptance и полной реализации C04. На тот момент prepare tool ещё не был реализован, MCP inventory включал семь tools. Текущая реализация и квалификация записаны в [evidence C04](C04.md); этот отчёт о guard не заменяет текущий статус.

Существующий RMCP stdio transport обёрнут в `StrictInput` до разбора данных. Guard отклоняет duplicate decoded JSON keys до чтения values, malformed frames, invalid UTF-8 и строки больше inclusive limit 65 536 bytes. Complete in-limit JSON frames, принятые guard, передаются побайтно, включая CRLF и поддерживаемые floating-point values. Пустые, ошибочные, oversized и частичные frames на EOF отбрасываются; после oversized frame guard синхронизируется на следующем LF, как provider. Синтетический error reply не добавляется.

Вход читается chunks по 4096 bytes; один poll обрабатывает максимум 32 processing steps и затем уступает выполнение. Лимит строки не означает peak memory 64 KiB: parsing и allocator добавляют расход. Точная верхняя граница peak RAM не установлена.

## Evidence

Узкий запуск `strict_input` прошёл: четыре теста (2020 filtered, 0.01 s) до переноса test module для строгого lint; перенос изменил только расположение тестов, не runtime code. Повторный строгий LINT-C прошёл после переноса. Static `strict_input_review` не выявил блокеров. Независимых повторных запусков не было.

STDIO-проверка duplicate key не наблюдала неожиданный ответ до следующего обмена `tools/list`. Это подтверждает только данный ограниченный сценарий и не доказывает, что произвольный отложенный ответ никогда не появится. Наблюдение subprocess дополняют source inspection и unit tests.

## Границы

На том историческом checkpoint это была только input validation транспорта, а не tool C04. `ak820_prepare`, plan digest/expiry/bindings и preparation/admission tests ещё не были реализованы, inventory содержал семь tools, а B11 acceptance ожидалась. Это историческое описание; актуальная [evidence C04](C04.md) отражает текущее состояние. В рамках проверки guard обращений к устройству или hardware не было.

**Итог:** ограниченная область документации/evidence подготовительной guard закрыта без оставшихся findings. Это не означает приёмку C04.
