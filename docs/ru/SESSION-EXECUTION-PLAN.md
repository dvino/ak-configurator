# Выполнение отдельными сессиями

Статус: контракты будущего исполнения, не реализация и не разрешение операций с устройством. Дата: 2026-11-01.
English: [SESSION-EXECUTION-PLAN.md](../SESSION-EXECUTION-PLAN.md).

Документ задаёт порядок и handoff для [BETA5-EXECUTION-PLAN.md](BETA5-EXECUTION-PLAN.md) (B) и [MCP-COMPLETION-PLAN.md](MCP-COMPLETION-PLAN.md) (M). Подробные алгоритмы, политика и issue acceptance находятся там. Карточки разделяют работу, но не заменяют и не ослабляют её условия. [RECOVERY-REVIEW-2026-11-01.md](RECOVERY-REVIEW-2026-11-01.md) фиксирует baseline и исправления ревью. Прежние широкие этапы и commit suggestions — группы работ, **не** задание реализовать всю группу за одну сессию.

## Правила новой сессии

Одна сессия выполняет ровно одну карточку. Начинать только после разрешения оператора на эту implementation/qualification session. Текущая плановая задача не запускает ни одной карточки. Общий контракт и выбранная карточка вместе являются готовым стартовым заданием; дополнительный выбор архитектуры исполнителю не передаётся.

1. Прочитать project AGENTS, requirements, PLAN, STATUS, recovery report, указанные B/M sections и все prerequisite handoffs. Зафиксировать code revision **и** digest проверенного dirty patch. Исходный `936a989a8fd7d8e24cbd70ddba0e768e3107494b` — provenance, не команда удалить последующую работу.
2. Использовать один собственный native worktree на основе проверенной работы `desktop-dev`. До изменений проверить branch/ownership по AGENT-ROUTING, сохранить concurrent/uncommitted work. Все writer cards выполняются последовательно в этом worktree. Совмещать можно только read-only investigation/review. Дерево firmware и OEM data не копировать в companion repository.
3. Назначить уникальный абсолютный `CARGO_TARGET_DIR`, не пересекающийся с чужими active builds, и записать его. Нужные результаты оставить следующим сессиям; broad cleanup запрещён. Команды ниже выполняются из этого worktree. Noisy safe local tests/build/lint при наличии RTK запускать через него с проверкой реального exit status. Remote/mutating/secret-handling операции не оборачивать.
4. Реализовать только delta карточки по уже выбранной архитектуре B/M. Тесты добавлять в binary inline modules и `scripts/test_mcp_stdio.py`, не создавать вымышленный library/integration target. Filtered tests должны находить и запускать ненулевое число нужных тестов. Новые имена тестов ниже — явный deliverable, не утверждение об их существовании.
5. Синхронно обновить EN/RU behavior/limits и metadata-only evidence. Default tests — offline; inherited UI против устройства не запускать. Unexpected device access/write, timeout, corrupt storage, bad readback или ownership loss останавливают операцию; никаких retry/reset/restore.
6. До commit/push получить независимое ревью. Сам план не разрешает commit/push. После substantive findings исправить, повторить affected checks и получить fresh review. Проверенный uncommitted patch с digest допустим как handoff; не коммитить молча ради названия этапа.
7. Завершиться на границе карточки. Не начинать следующую, чтобы использовать остаток контекста. Missing prerequisite оформляется blocked handoff. Незапланированный архитектурный/policy выбор требует остановки и вопроса, не расширения карточки. Не обещать фиксированное время для всех platform runs и external waits.

### Фиксированные проверки

`N(filter)` означает `cargo test --locked --bin ak-configurator <filter>`. Для `N(all)` filter опускается. `C(filter)` означает `cargo test --locked --features mcp-mock --bin ak-configurator <filter> -- --test-threads=1`; `C(all)` — без filter. Это обозначения точных команд, не новые scripts.

`STDIO`: пересобрать `cargo build --locked --features mcp-mock`, затем `python3 scripts/test_mcp_stdio.py --binary "$CARGO_TARGET_DIR/debug/ak-configurator"` (Windows `.exe`). Для production-feature mock denial отдельно собрать feature `mcp` без `mcp-mock` во втором isolated target directory и передать binary через `--production-binary`; реальные устройства не обнаруживать. Сохранить обе protocol generations существующего script.

`LINT-N`: `cargo fmt --check` и `cargo clippy --locked --all-targets -- -D warnings`. `LINT-C`: тот же fmt плюс `cargo clippy --locked --all-targets --features mcp-mock -- -D warnings`. `CAT`: `python3 .github/scripts/test_beta5_catalog.py`. Новые profile/prepare/execute filters добавляет соответствующая карточка до их использования.

### Обязательный handoff

При начале будущей реализации создать `docs/session-evidence/<ID>.md` и `docs/ru/session-evidence/<ID>.md` (это новые планируемые отчёты, не существующие test targets). Содержимое:

- ID, дата, worktree/branch, base revision, final revision либо dirty-patch digest, file inventory;
- prerequisite report IDs и точные input/catalog/firmware hashes; observed identity отдельно от package provenance;
- changed behavior и явно неизменённые safety boundaries;
- exact commands, exit codes, test names/counts, tool/platform versions, independent findings и закрытие;
- отдельные PASS/FAIL/PENDING для source/offline/protocol/platform/physical evidence;
- temporary resources, owner, purpose, in-use status, retained outputs и местонахождение private inputs **без содержимого и secrets**;
- один terminal result: `accepted`, `blocked` либо `needs-correction`; следующий card ID и точное unblock condition.

Не включать OEM bytes, settings/macros, private paths с secrets, raw device logs и full-ROM artifacts. Missing prerequisite блокирует dependents, не независимые карточки. Отчёт с pending external acceptance допустим, но accepted для карточки без её обязательных проверок — нет.

## Порядок и зависимости

Native default: **B00 → B01 → B02 → B03 → B04 → B05 → B06 → B07 → B08 → B09 → B10 → B11 → B12**.

MCP: **C00 → C01 → C02 → C03 → C04**; зависимости от B ниже обязательны. C00 можно запланировать сразу после B00, пока final firmware отсутствует, но не одновременно с writer. C02 можно выполнить после C00 до C01, если firmware не готова; C03 всё равно требует обе. Не дублировать QS/profile/backup engines в C. **C05 — отдельно разрешаемая optional mock work**, не prerequisite read-only MCP acceptance.

Offline qualification: **Q01 → Q02 → Q03 → Q04** после B12/C04; unavailable platform даёт pending evidence. Live **H01/H02/H03** — отдельные optional qualifications с собственным admission; Q не запускают их молча. Q04 сохраняет pending live issue closure, не выдумывает PASS. Это dependency graph, не разрешение параллельных writers.

## Карточки native implementation

### B00 — исходная точка исполнения

**Стартовое задание:** сохранить и проверить recovered review work, не реализовать feature. Inputs: принятые планы, recovery report, original issue snapshots. Files: PLAN/STATUS EN/RU и B00 reports. Прочитать `docs/AGENT-ROUTING.md`, проверить branch/status и recovered/catalog inventory. Привязать следующие reports к reviewed baseline/dirty-patch digest; original JSONL и unique material оставить вне репозитория. Через `cargo metadata --offline --no-deps --format-version 1` подтвердить binary/features и определить собственные worktree/output. Зафиксировать доступность final firmware, ничего не устанавливать. Проверить local links и `git diff --check`. **Выход:** ownership/provenance inventory и exact baseline; далее B01 либо C00. **Стоп:** unowned work, missing unique source или uncertain baseline.

### B01 — финальные pins и восстановление beta.5

**Стартовое задание:** выполнить только B stage 0. Нужны B00, final published firmware, successful release CI и отдельно validated private local OEM/native files. Files: `assets/ak820/beta5-*`, `.github/scripts/test_beta5_catalog.py`, B01 reports. Скачать exact final public inputs; записать tag/commit/run, sizes/digests, десять profiles и 200 directed pairs. Выполнить **реальную команду packer с шестью inputs из B stage 0**, новый private output для каждого профиля; IHX/map digests проверять отдельно от reconstructed permanent packed CODE. Full-ROM/recovery outputs оставить private; требуется 10/10. Добавить external pins/pairs checks в catalog test, выполнить CAT. **Выход:** immutable final-input inventory, 10/10 metadata proof и passing CAT; далее B02. **Стоп:** unpublished/failed candidate, missing private input, changed CLI, wrong count/hash или mismatch. К staged files не откатываться.

### B02 — QS v3 codec и offline migration

**Стартовое задание:** реализовать только QS/model часть B stage 1. Нужен B01. Files: `src/ak820_qs_versioned_offline.rs`, `src/ak820_settings_model.rs`, `src/ak820_typed_storage.rs`, `src/ak820_restore_plan.rs`, inline tests. Добавить V3, exact header, QSID23 u16 offset20/range1–1000, image-supplied default, CRC и forward-only offline migration. Добавить firmware-derived golden vectors и edge/corruption/property cases из B, без HID writer. Выполнить N(ak820_qs_versioned_offline), N(ak820_settings_model), N(ak820_restore_plan), LINT-N. **Выход:** codec/default/migration evidence и неизменный write denial; далее B03. **Стоп:** golden bytes/default расходятся с B01.

### B03 — точные beta.5 profiles/domain

**Стартовое задание:** завершить profile часть B stage 1. Нужны B01/B02. Files: новый `assets/ak820/beta5-profiles.json`, `src/ak820.rs`, `src/ak820_beta1_profiles.rs`, `src/ak820_settings_backup.rs`, affected exhaustive matches/tests. Добавить `ImageContract::Beta5(ProfileId)` и `beta5_firmware_matrix()` без generic registry; все десять profiles заполнить только из B01. Сохранить canonical Beta4 identity и beta.4/4.1 alias, отдельно package provenance и target. Добавить regressions с `beta5` в имени для positive/unknown/ambiguous/capability cases; N(beta5), N(ak820_settings_backup), CAT, LINT-N. **Выход:** exact read-only domain admission, ten-profile limits, no new writes; далее B04/C01. **Стоп:** unpinned field или unsupported identity inference.

### B04 — полное read-only reconciliation и backup metadata

**Стартовое задание:** выполнить domain/worker read path из B stage 2, без redesign страниц. Нужен B03. Files: `src/ui/ak820_connection.rs`, `src/ak820_backup_capture.rs`, `src/ak820_settings_backup.rs`, `src/ui/vial_hid_task.rs`. Читать QS23 через u16 API, сверить full NVM/macros/QS, передать source observation/alias, selected target, catalog digests и QS versions; сохранить generation/lease/fail-stop. Добавить v1/v2/v3, mismatched macro/QS, busy/disconnect/stale-generation fixtures. N(ak820_backup), N(ak820_connection), N(beta5), LINT-N. **Выход:** reusable verified snapshot и precise typed failures, без atomic-snapshot/restore claim; далее B05. **Стоп:** ambiguous read или implicit writer.

### B05 — ошибка snapshot и backup inspection UI (#2)

**Стартовое задание:** завершить offline UI/issue acceptance из B stage 2. Нужен B04. Files: `src/ui/ak820_backup_inspection.rs`, `src/ui/ak820_status.rs`, EN/RU catalogs. Сохранить original capture errors и fully verified preview; разделить LeaseBusy/OperationBusy/Disconnected/Failed. Добавить synthetic `ak820-settings-backup-2` inspection fixtures: compatible preview либо exact image/version/policy denial, не generic disconnect. Restore/reset оставить blocked. N(ak820_backup), N(ui::), LINT-N. **Выход:** offline #2 acceptance и явный Windows pending; далее B06. **Стоп:** старый backup нельзя безопасно интерпретировать; не объявлять его совместимым.

### B06 — ограниченный RGB worker (#3)

**Стартовое задание:** выполнить только B stage 3. Нужен B05. Files: `src/ui/rgb_settings.rs`, `src/ui/vial_hid_task.rs`, affected result handling/tests. Вынести synchronous effect/brightness/color/speed HID и autosave из egui; ровно одна submitted command и один replaceable unsent draft, без replay/cancel отправленного. До submit проверить exact image/ranges/authorization; beta.5 production write denial сохраняется. Инъецировать delay/fault/disconnect и доказать responsive close/Quit, bounded queue. N(rgb), N(vial_hid_task), N(ui::), LINT-N. **Выход:** offline responsiveness/fail-stop proof; H02 отдельно gated; далее B07. **Стоп:** исправление требует расширения write permission.

### B07 — конечная unlock operation (#4)

**Стартовое задание:** выполнить только B stage 4. Нужен B06. Files: `src/vial/unlock.rs`, `src/ui/vial_hid_task.rs`, tests. Сохранить exact ownership/identity, already-unlocked success; добавить bounded states/deadline из B, terminal errors и cancellation без retry loop. Проверить busy helper, timeout, denial, disconnect; N(vial_unlock), N(unlock), N(ui::), LINT-N. **Выход:** отсутствие indefinite polling, offline transition evidence; H03 pending; далее B08. **Стоп:** unknown device либо physical unlock requirement в automatic test.

### B08 — multi-frame indicator/HID-loan stability (#5)

**Стартовое задание:** выполнить только B stage 5. Нужен B07. Files: `src/ui/ak820_os_indicator.rs`, `src/ui/device_connection.rs`, existing inline egui tests. Сохранить Job::Sample/Write generation/freshness и cached loan facts. Добавить 100 steady и 100 alternating EN/RU multi-frame regressions для version, Backups page/dialog/capture, connection availability и generation; инъецировать stale completion/competing work. Не выдумывать physical root cause и не менять saved mappings. N(ak820_os_indicator), N(ui::), LINT-N. **Выход:** frame assertions и граница partial/live evidence; далее B09. **Стоп:** исправление включает indicator writes или сбрасывает page ownership.

### B09 — оставшиеся navigation/backup entry gaps (#6)

**Стартовое задание:** выполнить только матрицу B stage 6. Нужен B08. Files: `src/ui/ak820_status.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/app_settings.rs`, tests `src/ui/layout_settings_dropdown.rs`, EN/RU catalogs. Сохранить persistent native pages, один native AppSettings service route и Wasm-only menu. Добавить source observation/alias отдельно от target, inspection/blocked-restore first steps, safety/back/cancel и точное RU editor/import/export explanation. Не удалять уже отсутствующий native duplicate, Wasm behavior не менять. N(ui::), LINT-N; evidence по всем шести строкам. **Выход:** offline matrix, без redesign navigation; далее B10. **Стоп:** provenance выводится из byte-identical firmware или выбранной цели.

### B10 — detected layout отдельно от LED mappings (#7)

**Стартовое задание:** выполнить только B stage 7. Нужен B09. Files: `src/ui/ak820_os_indicator.rs`, tests, EN/RU catalogs. Сохранить partial existing improvements; разделить detected EN/RU, saved EN/RU LED choices и paused/mode state. Пояснить неизменность mappings при pause; тестировать EN detected с EN=Off/RU=On, unknown layout, disabled/paused helpers и target change без writes. N(ak820_os_indicator), N(ui::), LINT-N. **Выход:** semantic regressions; exact beta.5 physical LED observation pending H01; далее B11. **Стоп:** inherited helper пишет при observation.

### B11 — installer release selection/admission

**Стартовое задание:** выполнить только B stage 8. Нужны B01/B03/B10. Files: `src/ak820_release.rs`, `src/ui/ak820_install.rs`, `src/ak820_install_plan.rs`, `src/ak820_install_executor.rs`, admission tests. Заменить beta4_1 boolean на выбранный InstallerRelease enum, обобщить общий admission, сохранить Beta3/Beta4_1 semantics и source aliases. Beta5 default требует prerelease opt-in и final pins. Проверить 200 allow pairs и beta3/unknown/downgrade/digest/catalog/opt-in denial; settings restore denied. N(ak820_release), N(ak820_install), N(beta5), CAT, LINT-N. **Выход:** immutable installer preflight без physical flash; далее B12/C04 prerequisites. **Стоп:** missing pin/confirmation content; никаких physical loader/erase.

### B12 — native integration regression и документация

**Стартовое задание:** выполнить offline integration часть B stage 9. Нужны принятые B02–B11. Files: affected tests и matching README/INSTALL/REQUIREMENTS/PLAN/STATUS/release notes EN/RU, без новой feature. Один раз выполнить N(all), LINT-N, CAT на integrated baseline; проверить local Markdown links и `git diff --check`. Проверить issue coverage, snapshot/alias labels, release defaults и write denials. Записать counts/failures, получить independent review реализации и evidence. **Выход:** reviewed native offline candidate и platform/hardware checklist; далее Q, когда C04 тоже готов. **Стоп:** regression либо claim без live evidence. #2–#5/#7 не закрывать offline-тестами.

## Карточки MCP implementation

### C00 — текущие CLI/tools и stdio/error coverage

**Стартовое задание:** выполнить только M0 protocol/CLI/reporting без final-image claims. Нужен B00. Files: `src/mcp/{mod,backend,dto,mock,trace}.rs`, `scripts/test_mcp_stdio.py`, явно новые dev-only `scripts/mcp-interop/{package.json,package-lock.json,check.mjs}`, MCP EN/RU docs. Сохранить шесть реальных tools и defaults CLI; расширить script для обеих protocol generations, malformed/oversized input, invalid params, backend/access conflicts, shutdown, stderr/stdout и bounded cancellation. Создать official TypeScript SDK client и pinned Inspector wrapper по M0 step 5, с locked mutually compatible versions, integrity metadata и prepared cache; это единственный implementation owner independent-client coverage. Без новых production dependencies, GUI/device startup и downloads в normal tests. C(all), STDIO, LINT-C, затем `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop` и `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`. Записать versions/lock digest и SDK/Inspector results. **Выход:** reviewed host protocol evidence; exact-image и three-platform P0 pending; далее C01 либо C02. **Стоп:** замена session/test/trace API или ослабление admission.

### C01 — exact bootstrap fixture и shared ownership

**Стартовое задание:** завершить M0 exact-image/ownership. Нужны B01/B03/B04/C00. Files: `src/mcp/backend.rs`, inline fixtures/tests; `src/hid.rs`, `src/hid_ak820.rs` только для reproduced shared ownership defects. Добавить positive full packed-bootstrap fixture по B01, без OEM/full-ROM в repository. Проверить FD00→FD01, отсутствие probes unrelated VID/PID, ambiguous denial, busy/cancel/disconnect и bounded owner release. C(all), STDIO, LINT-C. **Выход:** exact fixture proof, platform P0 ждёт Q01–Q03. **Стоп:** synthetic/linked fingerprint вместо trust либо невоспроизведённый ownership flaw.

### C02 — bounded private artifact store

**Стартовое задание:** реализовать только M private-artifact contract. Нужен C00; storage tests не требуют final firmware. Files: новый `src/mcp/artifacts.rs`, module registration и inline tests. Реализовать root, random IDs, payload/manifest schema, permissions/no-follow, byte/count limits, explicit eviction и atomic/durable storage policy. На temporary synthetic data проверить traversal/symlink, truncation, limits, collisions, full disk/fault cleanup и platform durability limitations. Добавить имена `mcp_artifacts`; C(mcp_artifacts), LINT-C. **Выход:** reusable store без new tool/device access; далее C03 после C01. **Стоп:** ОС не обеспечивает изоляцию; privacy не ослаблять и auto-evict не добавлять.

### C03 — shared verified backup tool

**Стартовое задание:** выполнить adapter часть M1 через C02 store. Нужны B04/B05/C01/C02. Files: `src/mcp/backend.rs`, `src/mcp/dto.rs`, existing shared backup engine, stdio script/tests. Добавить R `ak820_backup`, переиспользовать native reconciliation, source alias/provenance, generation; возвращать metadata/artifact ID. Добавить `mcp_backup` tests для equality с native fixture, v3/u16, contention, mismatch/storage fault/cancel/disconnect. Expected inventories stdio и official-SDK/Inspector изменить с шести на семь. C(mcp_backup), N(ak820_backup), STDIO, LINT-C. **Выход:** byte-identical verified snapshot, no writer; далее C04. **Стоп:** второй HID engine или raw settings leak.

### C04 — immutable read-only prepare

**Стартовое задание:** выполнить только M2. Нужны B11/C03. Files: shared installer/plan modules, `src/mcp/{backend,dto}.rs`, store integration, stdio script/tests. Добавить `ak820_prepare`, canonical versioned digest, strict fields/duplicate-key denial, expiry, source/backup/catalog binding. R/L доступны с реальным device-read-only CLI; S/T/D/production execute denied. Добавить `mcp_prepare` mutation/200-pair/admission tests, stdio и official-SDK/Inspector tool inventories — восемь. C(mcp_prepare), C(all), STDIO, LINT-C. **Выход:** eight-tool read-only candidate без approval/writer; далее Q01. **Стоп:** restore authority из installer pair или необходимость нового access-mode design.

### C05 — optional approval-bound mock execute

**Стартовое задание:** выполнить M3 только **после отдельного явного разрешения разрабатывать этот mock writer path**. Нужны C04 и разрешение; для read-only acceptance не требуется. Files: shared plan/approval state, MCP adapter/mock tests. Привязать one-shot local approval к plan/source/generation/expiry, запретить client mint. Добавить `mcp_execute` replay/timeout/cancel/uncertain-result cases, terminal report и no retry. Mock inventory — девять; HID — восемь, direct execute denied. C(mcp_execute), C(all), STDIO, LINT-C. **Выход:** mock-only architecture evidence, не production/hardware qualification. **Стоп:** missing authorization либо attempted device write/policy extension.

## Карточки platform/physical qualification

### Q01 — Linux offline/process и artifacts

**Стартовое задание:** проверить reviewed integrated B12/C04 candidate на Linux, без feature changes. Files: Q01 evidence, installation/MCP docs. N(all), C(all), STDIO, LINT-N/C, CAT в isolated outputs. Выполнить cached independent-client recipe из C00: `npm ci --offline --ignore-scripts --prefix scripts/mcp-interop`, затем `node scripts/mcp-interop/check.mjs --binary "$CARGO_TARGET_DIR/debug/ak-configurator"`; official-SDK handshake/read/status и Inspector discovery/tools-list должны пройти на final candidate, с versions/lock digest/negotiated protocol в отчёте. Custom harness их не заменяет. Далее bounded pipe/Unicode/shutdown/cancel и permission-error fixtures. Проверить Linux x86_64/arm64 package matrix, SHA256/size и extraction/launch instructions по реальным artifacts; не заявлять unrun architecture launch. UI smoke — только disconnected/offline-safe. **Выход:** Linux process results и artifact evidence; далее Q02. **Стоп:** unavailable runner/artifact, unreviewed source или unexpected HID startup; pending, не code change.

### Q02 — Windows offline/process и artifacts

**Стартовое задание:** повторить bounded offline recipes Q01 на Windows с `.exe`/isolated targets, без device access. Нужны B12/C04; Q01 — default predecessor, не portability substitute. Проверить x86_64/arm64 packages/checksums/instructions, pipe/Unicode/exit и mocked hidapi lease/error mapping. Unavailable architecture указать отдельно. **Выход:** Windows stdio/artifact evidence, live issues ещё открыты; далее Q03. **Стоп:** unavailable platform/runner или runtime failure; исправление возвращается к owning card в новой сессии.

### Q03 — macOS offline/process и artifacts

**Стартовое задание:** повторить bounded offline/process tests на macOS и permission-error fixtures, без device access. Нужны B12/C04, default Q02 handoff. Проверить x86_64/arm64 artifacts, extraction/launch instructions, stdout/pipe/Unicode/cancellation/exit; unavailable launch не равен build success. **Выход:** macOS stdio/artifact evidence; далее Q04. **Стоп:** missing runner/artifact или failed required test; Linux/Windows не доказывают macOS.

### Q04 — cross-platform security, release docs и final review

**Стартовое задание:** собрать M4/B stage 9 evidence без реализации. Для full three-platform read-only completion нужны accepted Q01/Q02/Q03; раньше допускается только явно pending report. Files: final reports, release notes, README/INSTALL/MCP/STATUS EN/RU. Проверить path/HID/shell escape, raw-data leak, schema/size/rate bypass, concurrent writers, approval escalation/replay, uncertain retry. Сверить шесть application targets/checksums, platform test counts, offline/live состояние issue и deferred C05. Проверить links/diff, получить fresh independent review. **Выход:** accepted read-only MCP только при M0–M2/M4, official-SDK/Inspector interoperability Q01 и трёх stdio platforms, иначе precise pending milestone. H не блокируют честное offline completion, но блокируют соответствующее live issue closure. **Стоп:** незакрытое security finding или missing required platform evidence; не объявлять все P1/P2 complete.

### H01 — отдельно допущенное Windows read-only observation

**Стартовое задание:** проверить live #2/#5/#7 только после B12/Q02, exact connected beta.5 identity и явного допуска proven read-only launch/observation. Не требуются key presses, rotation, reconnect, unlock или option writes. До launch проверить inherited auto-writing helpers, оставить выключенными, если они могут писать. Собрать snapshot/preview-or-specific-denial, version/Backups timing при steady и EN/RU layout changes, detected/mapped/paused LED observation для exact firmware. Изменение OS layout не является simulated device input. Если active indicator behavior нельзя наблюдать безопасно, оставить pending. **Выход:** per-issue read-only evidence/limitation, не общее closure claim. **Стоп:** setter, manual interaction requirement, identity mismatch, lease/timeout/storage/readback fault; без retry/reset/restore.

### H02 — gated Windows RGB live qualification (#3)

**Стартовое задание:** не выполнять, пока beta.5 write policy запрещает операцию. Нужны B06/Q02, exact image support, явный release-policy admission и fresh authorization на конкретную RGB operation. Только затем собрать bounded responsiveness/Quit evidence и exact write budget по B stage 3; без replay/autosave вне approved contents. **Выход:** конкретное #3 physical/protocol evidence, не general write qualification. **Стоп:** policy false, missing authorization либо первый uncertain/error result. До допуска deliverable — blocked qualification report, не device test.

### H03 — отдельно авторизованная Windows unlock qualification (#4)

**Стартовое задание:** нужны B07/Q02 и explicit authorization exact unlock operation на admitted image. Сначала shared owner и already-unlocked. Наблюдать bounded B stage 4 state machine без retries/unrelated writes; не запрашивать неразрешённую manual key sequence. **Выход:** exact Windows unlock/termination evidence либо pending limitation. **Стоп:** missing authorization, physical interaction requirement, ownership conflict, timeout/failure. Restore/loader/flash этим не разрешаются.

## Стартовый текст следующему агенту

> Выполни только карточку `<ID>` из docs/ru/SESSION-EXECUTION-PLAN.md. До изменений прочитай общий контракт, prerequisite handoffs и связанные B/M steps. Проверь baseline и исключительное владение worktree/build-output. Выполни уже выбранные действия карточки и точные check recipes, сохрани device/write gates и закончи EN/RU evidence handoff. Не начинай другую карточку, не делай commit/push без разрешения и ревью, не придумывай missing architecture/input. При blocker сохрани конкретный unmet gate и задай только необходимый вопрос. Задание действует, только когда оператор отдельно разрешил исполнение этой карточки.

Вместо `<ID>` подставляется одна следующая admitted card из предыдущего handoff, не вся B/M группа. Новый агент читает repository/reports, а не наследует огромный chat transcript.
