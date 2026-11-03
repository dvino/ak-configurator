# Завершение интеграции MCP

Статус: исполнительный план, не подтверждение реализации. Дата: 2026-11-01.
Исходный проектный контракт: [MCP_IMPLEMENTATION_PLAN.md](MCP_IMPLEMENTATION_PLAN.md). Этот документ уточняет его для текущего P0 и beta.5; при расхождении действуют более строгие ограничения безопасности.

## 1. Результат и границы

Завершённой считается интеграция, когда stdio MCP проходит offline protocol/integration tests на Linux, Windows и macOS, использует тот же точный image/operation engine, что и native UI, поддерживает чтение, backup, immutable prepare и явно авторизованный execute, не допускает параллельных HID writer и имеет закрытые security/admission тесты.

В текущем beta.5 `settings_writes_authorized=false`. Поэтому завершение интеграции **не включает** запись QS, макросов, layout, RGB, AKBT, reversible test suites или protected update. Эти scopes остаются запрещёнными будущими расширениями до отдельного release authorization. Не объявлять P1/P2 полностью выполненными, если реализован только read-only subset.

MCP не является отдельным способом обойти UI authorization, device lease, packed identity или physical confirmation. Никаких arbitrary path, arbitrary HID bytes, shell, telemetry, удалённого вызова, automatic retry и background startup.

## 2. Что уже реализовано и что не переделывать

Сохранить существующие решения `src/mcp/mod.rs`, `backend.rs`, `dto.rs`:

- opt-in `--mcp-stdio`; необязательные `--mcp-access=read-only` и `--mcp-backend=hid` соответствуют текущим defaults; `--mcp-backend=mock` требует feature `mcp-mock`. Не придумывать `--read-only`, `--hid`, `--mock` или scope flags;
- stdio server на `rmcp 3.8.0`, JSON-RPC frames до 64 KiB;
- шесть tools: `ak820_devices`, `ak820_session`, `ak820_read`, `ak820_tests`, `ak820_trace`, `ak820_operation`; сохранить actions, resource behavior, closed schemas и result envelope. Полное layout/settings reconciliation — новая доменная интеграция, а не существующие read-layout/read-settings tools;
- один bounded `sync_channel(1)`, один operation owner, cached operation/report IDs;
- strict closed DTO, отказ от settings/macros/payload trace;
- FD00-first identity затем FD01, metadata trace, отмена ожидающей/читающей операции;
- irreversible process guard: после реального device access нельзя переключиться в mock;
- отсутствие automatic network, writes и firmware payload.

Не добавлять второй backend, thread-per-tool или собственный JSON parser. Не копировать HID protocol из UI в MCP.

## 3. Целевая архитектура

### 3.1 Общий operation engine

Вынести из backend/UI общий сервис с интерфейсами:

```text
DeviceDiscovery → ExactIdentity → ReadSnapshot
ReadSnapshot → BackupArtifact | ImmutablePlan
ImmutablePlan + Approval + FreshIdentity → ExecuteOperation
ExecuteOperation → TerminalOperationReport
```

MCP DTO только проецирует доменные результаты. Точный `ImageContract`, QS v1/v2/v3, CODE digest, macro getter, lease, generation и fail-stop остаются в домене. Backend не должен знать offsets QSID или layout bytes.

### 3.2 Девять инструментов

Добавить к шести:

- `ak820_backup`: read-only захват проверенного snapshot в private artifact store;
- `ak820_prepare`: offline/read-only создание immutable plan из выбранного backup/target;
- `ak820_execute`: выполнение только поддерживаемого плана с одноразовым approval.

Имена и schemas закрыты; шесть существующих tools не переименовывать и не заменять. Существующие actions `status/cancel` у `ak820_operation` охватывают новые операции; list/get не добавлять. Сохранить result envelope; operation-bearing responses добавляют ID, generation, terminal state, domain error и safe diagnostics. Девять schemas — целевой inventory; production публикует только разрешённые tools (восемь после M2), mock-only execute не публикуется на HID. Не возвращать OEM bytes, settings defaults, macro payload или secrets в обычном tool result.

### 3.3 Access scopes

Сохранить модель R/L/S/T/D:

- **R** — discovery, identity, layout/settings read, backup;
- **L** — prepare из локальных artifacts, без device mutation;
- **S** — saved settings; для beta.5 запрещён;
- **T** — reversible test suites; пока запрещён;
- **D** — destructive/protected update; пока запрещён.

Текущий CLI не имеет scope-selection flag. Сохранить `--mcp-access=read-only`: он запрещает device mutation, не ограниченное private local storage. В P0 доступен R; M1 добавляет R backup, M2 добавляет L prepare при той же access policy. Для prepare не нужен writer access mode; scope parser в этом release не добавлять.

| Admission | P0 | После M1 | После M2 | HID mutation |
|---|---|---|---|---|
| R discovery/read/test/trace | Существующие tools | Сохранены | Сохранены | Никогда |
| R backup, local artifact creation | Ещё не реализован | Разрешён | Разрешён | Никогда |
| L local artifact/prepare | Ещё не реализован | Ещё не реализован | Разрешён | Никогда |
| S/T/D, production execute | Denied | Denied | Denied | Denied |

L принимает только artifact ID, не arbitrary path. Скрывать execute из HID tool list и отклонять прямой вызов стабильным `authorization_denied`. M3 допускает только feature-gated mock execution после отдельного разрешения на разработку; HID admission не меняется.

### 3.4 Immutable plan и approval

`prepare` связывает: exact source CODE SHA256, target release/profile, catalog/matrix/capability digests, QS source/target versions, backup digest, operation kind, limits и expiry. Любое изменение входа делает plan invalid, а не молча пересчитывает его.

`execute` принимает только `plan_id`, `approval_id`, `expected_generation`, `expected_source_code_sha256`. Approval одноразовый, короткоживущий, привязан к plan digest и пользовательскому подтверждению. MCP client не может сам создать approval через tool. Для beta.5 production execute остаётся недоступен; offline mock execute не считается device qualification.

### 3.5 Владение устройством и завершение

Единый per-device owner разделяется с UI. Состояния: `Ready`, `LeaseBusy`, `OperationBusy`, `Disconnected`, `Failed`. До начала execute требуется lease и повторная exact identity. При contention не ждать бесконечно и не steal lock.

Все операции имеют terminal `Succeeded|Failed|Cancelled|TimedOut`. Cancel до submit не отправляет команду; cancel после отправки прекращает ожидание только если доменный протокол это поддерживает и не утверждает откат. После uncertain write никаких повторов. Disconnect/shutdown не запускает reset/restore.

### 3.6 Private artifact store

Создать `src/mcp/artifacts.rs`. Корень: `dirs::data_local_dir()/ak-configurator/mcp-artifacts/v1`; при отсутствии per-user data directory вернуть явную ошибку, не использовать current directory. Каждый random 128-bit lowercase-hex ID владеет каталогом с `payload.bin` и `manifest.json`. Manifest schema `ak820.mcp.artifact/v1` хранит kind, created UTC, payload length/SHA256, source CODE/release, QS version, completeness и plan expiry; arbitrary user path отсутствует. Максимум backup payload 16 MiB, plan payload 1 MiB, manifest 64 KiB, хранится не более 100 artifacts; eviction выполняется только явно по artifact ID, не автоматически.

Каталоги 0700, файлы 0600, `O_NOFOLLOW`/platform equivalent, пути только из server-generated IDs. Записать temporary file в том же каталоге, flush + `sync_all`, close, atomic rename, затем `sync_all` payload directory и manifest directory на Unix; на Windows сбросить file buffers и применить atomic replace, а неподдерживаемый directory flush указать как durability limitation, не success evidence. При любой ошибке удалить только новый temporary/incomplete artifact и вернуть storage error. Backup содержит raw private data; tool result получает metadata/digest. Trace/report по умолчанию metadata-only. Экспорт — отдельная пользовательская операция с явным путём, не MCP arbitrary path.

Границы сессий и готовые задания находятся в [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md): C00/C01 разделяют M0, C02/C03 — storage/backup, C04 — prepare, отдельно gated C05 — mock execute, Q01–Q04 — acceptance. Общая B-работа является prerequisite, не дублируется.

## 4. Этап M0 — завершить и зафиксировать P0

**Файлы:** существующие `src/mcp/{mod,backend,dto,mock,trace}.rs`, `src/hid.rs`, `src/hid_ak820.rs`, `scripts/test_mcp_stdio.py`, `docs/MCP*`. Расширять inline unit tests и subprocess script, не придумывать library/integration targets. Команды выполняются в собственном checkout с изолированным `CARGO_TARGET_DIR`; на Windows binary имеет суффикс `.exe`.

1. Добавить positive beta.5 full packed bootstrap fixture из финальных pinned inputs после stage 0 beta.5 plan. Не использовать synthetic fixture как production trust.
2. Проверить FD00→FD01 fallback, отсутствие probe других VID/PID, ambiguous identity denial и exact CODE admission.
3. Исправить только воспроизведённые gaps live ownership, helper failure, cancellation и disconnect. Тесты должны доказать отсутствие concurrent read/write и завершение операции.
4. Расширить report безопасными timings/stages/error identity; payload trace оставить выключенным.
5. Protocol tests: initialize, tools/list, tools/call, malformed JSON, oversized 64KiB+1 frame, unknown method/tool, invalid params, notification, shutdown. Проверить stdout только protocol, logs stderr. C00 также создаёт явно новые dev-only `scripts/mcp-interop/{package.json,package-lock.json,check.mjs}`: официальный TypeScript MCP SDK client и pinned MCP Inspector, не ещё один handwritten client. Один раз в C00 разрешить exact mutually compatible versions, записать versions/integrity в lockfile/report и подготовить controlled cache. `check.mjs --binary <mock-binary>` выполняет official-SDK stdio handshake/discovery, tool schema/list, safe fixture read/operation-status call и Inspector CLI discovery/tools-list против того же mock. Сохранить реальную status/cancel schema; без GUI, network service и HID backend. Все transitive inputs pinned; floating npx/download в normal tests запрещён. Offline запуск: `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop`, затем `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`. Wrapper — новый interoperability deliverable, не существующий Rust test target. Missing cache или отсутствие совместимого official client — failed gate, не разрешение заменить его handwritten harness. Q01 запускает проверку final candidate; C03/C04 обновляют expected inventory на семь/восемь tools.
6. CLI matrix: один `--mcp-stdio` задаёт read-only HID по умолчанию, но не запускает discovery до запроса; явные access/backend дают тот же admission. Unknown flags/access modes и duplicate/conflicting backend/access отклоняются. Feature `mcp` без `mcp-mock` запрещает mock; development mock проверяется без HID. Сохранить irreversible real→mock guard. Forbidden writes проверять без setters. GUI startup без flags не является offline acceptance test.
7. Команды:

```bash
cargo test --locked --features mcp-mock --bin ak-configurator mcp:: -- --test-threads=1
cargo build --locked --features mcp-mock
python3 scripts/test_mcp_stdio.py --binary "$CARGO_TARGET_DIR/debug/ak-configurator"
python3 .github/scripts/test_beta5_catalog.py
cargo clippy --locked --all-targets --features mcp-mock -- -D warnings
```

Для mock denial отдельно собрать binary с feature `mcp` без `mcp-mock`, в другом `CARGO_TARGET_DIR`, и передать его script через `--production-binary`. `CARGO_TARGET_DIR` задаётся явно для сессии; ноль совпавших filtered tests — не PASS.

**Gate:** P0 нельзя считать завершённым без positive exact fixture и Windows/macOS/Linux stdio results. Пока доступна только Linux, зафиксировать остальные как pending, но продолжать offline M1 можно.

## 5. Этап M1 — общий snapshot/backup для beta.5

**Зависимости:** stage 2 beta.5 execution plan. **Файлы:** общий backup engine, `src/mcp/backend.rs`, `src/mcp/dto.rs`, artifact-store module/tests.

1. Подключить MCP к тому же full NVM/macro/QS reconciliation, что и native backup. Добавить QS v3/QSID23 u16 и exact beta.5 image contract только через доменный API.
2. Реализовать `ak820_backup` как R operation. Вернуть artifact ID, observed source/CODE digest, QS version, completeness и safe error; не возвращать raw settings. Сохранить canonical Beta4 с явным alias byte-identical beta.4/4.1; receipt-based package provenance отделено от device observation.
3. Реализовать private artifact store, атомарную запись temp→rename, fsync policy, ограничение размера и отказ на storage fault. Не делать automatic cleanup чужих/активных artifacts.
4. Проверить generation: reconnect между read и сохранением не смешивает данные. Busy UI и busy MCP различаются; disconnect не выдаётся за lease busy.
5. Тесты: beta.4.1/beta.5 snapshots, v1/v2/v3, macro mismatch, QSID23 mismatch, corrupt storage, symlink/path traversal, concurrent backup, cancel/disconnect, stale generation.
6. Добавить inline regressions с `mcp_backup` в имени; выполнить `cargo test --locked --features mcp-mock --bin ak-configurator mcp_backup`, `cargo test --bin ak-configurator ak820_backup` и M0 stdio script после пересборки mock binary. Tool-list expectation в script — семь после M1 и восемь только после завершения M1+M2.

**Приёмка:** backup byte-for-byte соответствует native domain snapshot для одного fixture; ни один writer не добавлен.

## 6. Этап M2 — immutable prepare

**Зависимости:** M1 и stage 8 beta.5 plan. **Файлы:** shared plan module, `src/mcp/{backend,dto}.rs`, plan fixtures/tests.

1. Добавить `ak820_prepare` только для поддерживаемых offline/install plans. Для settings restore возвращать `settings_writes_authorized=false`, даже если catalog pair существует.
2. Сериализовать plan в канонический JSON с версией schema, digest и всеми exact identities. Отклонять unknown fields и duplicate keys.
3. Проверить 200 installer pairs и deny beta.3/unknown/downgrade/digest mismatch. Не помещать packed payload в tool response.
4. Проверить expiry, source/backup binding и отказ при смене catalog/target/backup. Plan нельзя редактировать через execute params.
5. Тесты canonical digest, tampering, unknown schema, expiry, cross-device binding, storage failure, all transition pairs.
6. Добавить inline regressions с `mcp_prepare` в имени; выполнить `cargo test --locked --features mcp-mock --bin ak-configurator mcp_prepare` и M0 stdio harness после пересборки. Ноль совпадений — не PASS; проверить R/L admission с реальным read-only CLI.

**Приёмка:** prepare полностью offline/read-only, не открывает HID writer и не создаёт approval.

## 7. Этап M3 — явно авторизованный execute, пока только в mock

**Зависимости:** M2 и отдельное разрешение на разработку writer path; физическая операция не разрешена этим планом.

1. Реализовать execution state machine и `ak820_execute` за feature/scope gate. Для beta.5 production возвращать stable denial до release policy change.
2. Approval выдаёт существующий native/local confirmation flow, не MCP tool. Привязать к plan digest, source CODE, generation и одному использованию.
3. До loader/erase повторно проверить все digests и confirmation contents. Для firmware install сохранить fresh Esc+left Ctrl и flash-once/readback правила.
4. В mock проверить success, denied, expired, reused approval, wrong generation/source, disconnect, timeout, cancel before/after submit и uncertain result. После failure не должно быть retry.
5. `ak820_operation` с action `status` сохраняет доступ к terminal report после client disconnect; action `cancel` не утверждает rollback. Новый client не может изменить чужой approval; list/get actions не добавлять.
6. Добавить inline regressions с `mcp_execute` в имени; выполнить `cargo test --locked --features mcp-mock --bin ak-configurator mcp_execute` и M0 stdio harness после пересборки. Mock tool inventory — девять только в этом авторизованном этапе; HID inventory остаётся восемь.

**Gate:** не переводить mock execute в production без отдельного решения и независимого security review. Physical tests — отдельная авторизация.

## 8. Этап M4 — platform, security и эксплуатационная приёмка

1. На Linux/macOS/Windows проверить stdio framing, Unicode, broken pipe, shutdown, cancellation и process exit без зависших threads. Использовать изолированные `CARGO_TARGET_DIR`. Q01 дополнительно запускает cached official-SDK client и pinned Inspector checks из C00 на Linux; записать client/Inspector/Node/npm versions, lockfile digest и negotiated protocol. One-platform independent interoperability обязательно дополнительно к custom subprocess harness и three-platform stdio.
2. На Windows проверить hidapi lease/error mapping; на macOS permissions; на Linux udev. Permission denied не считать disconnected.
3. Провести security checklist: no arbitrary paths/HID/shell; no secrets/OEM logs; closed schemas; size/rate bounds; no concurrent writer; no scope escalation; no approval forgery/replay; no retry after uncertain result.
4. Запустить полный offline suite, doc-link check, formatting/clippy. Зафиксировать versions и test counts.
5. Обновить `docs/MCP.md` и RU: фактически поддерживаемые tools/scopes, запуск, limitations и evidence. Не переносить будущие S/T/D в список поддерживаемых.
6. Независимый reviewer проверяет implementation против исходного P0–P2 contract и этого плана. Если S/T/D остаются намеренно deferred, общий статус — «read-only MCP complete; write scopes deferred», а не «все MCP requirements complete».

## 9. Порядок коммитов

1. `mcp: complete read-only protocol and exact beta.5 coverage`
2. `mcp: share verified beta.5 backup snapshots`
3. `mcp: add immutable operation preparation`
4. `mcp: add approval-bound mock execution`
5. `docs: record MCP platform and security evidence`

Каждый коммит — после независимого ревью. Не смешивать MCP writer с общим beta.5 QS implementation; сначала доменный контракт, затем тонкий MCP adapter.

## 10. Критерий завершения текущей задачи MCP

Implementation milestone на доступных platforms может быть завершён при pending остальных platforms, но это ещё не accepted read-only MCP completion. Для полной приёмки обязательны M0–M2, security/operational проверки M4, cached official-SDK/Inspector interoperability results из Q01 и stdio-результаты на всех трёх platforms; backup/prepare не содержат writer, все unsupported scopes стабильно denied, документация и independent review закрыты. M3 обязателен для полной архитектурной готовности execute, но его production включение и S/T/D не входят в текущий release. Отсутствие physical evidence явно остаётся limitation.
