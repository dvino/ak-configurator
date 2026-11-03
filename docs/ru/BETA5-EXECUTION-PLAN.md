# AK Configurator: исполнительный план beta.5

Статус: утверждённый к исполнению порядок работ; реализация ещё не начата.
Дата снимка: 2026-11-01. Основная ветка: `desktop-dev`.
Исходная точка кода: `936a989a8fd7d8e24cbd70ddba0e768e3107494b` плюс восстановленные артефакты предыдущей сессии.

Происхождение переноса и сопоставление issue с кодом зафиксированы в [RECOVERY-REVIEW-2026-11-01.md](RECOVERY-REVIEW-2026-11-01.md).

Этот документ заменяет проектирование «на ходу». Исполнитель выполняет этапы по порядку, не расширяет объём и не объявляет аппаратную проверку без физической клавиатуры. План охватывает открытые issue #2–#7, QS record v3 и установку beta.5. Каждый этап завершается отдельным коммитом только после независимого ревью; без ревью изменения остаются незакоммиченными.

Порядок разделён в [SESSION-EXECUTION-PLAN.md](SESSION-EXECUTION-PLAN.md): B00–B12 — ограниченные implementation sessions, C используют общий domain, Q/H разделяют offline platform и допущенное physical evidence. Если широкий этап содержит несколько карточек, не выполнять его целиком в одной сессии.

## 1. Непреложные правила

1. Работать в `desktop-dev`; общие изменения затем переносить в `main` обычным merge, без копий веток.
2. Один исполнитель владеет перечисленными в этапе файлами. Не запускать параллельные сборки с общим `target/`. Изолированный worktree использует собственный `CARGO_TARGET_DIR`.
3. Не копировать дерево `dvino/ak820` или OEM ROM/recovery в репозиторий. Не менять прошивку в рамках этих задач.
4. Не выполнять запись, сброс, loader transition или flash на физическом устройстве без отдельного разрешения на конкретную операцию. Автоматические проверки — offline/read-only, без нажатий, вращения, reconnect и unlock.
5. Точный `code_sha256` packed-образа отличается от linked CODE fingerprint. Каталог, имя профиля, fixture и linked fingerprint не допускают образ к работе.
6. До финальной публикации beta.5 не назначать новые digest константы. Старые staged `assets/ak820/beta5-*.json` — исторический снимок superseded release, не текущий production pin.
7. После ошибки записи, timeout, storage fault или readback mismatch — остановиться, сохранить неопределённое состояние, не повторять запись и не делать reset/restore.
8. `settings_writes_authorized=false` остаётся запретом на запись настроек. Флаги переходов в catalog не создают право production backup/restore.
9. Для каждого изменения поведения, формата, команды и ограничения синхронно обновить EN/RU документы.
10. Cargo объявляет binary, не library. Unit/headless tests запускать через `cargo test --bin ak-configurator <filter>`. Новые frame regressions добавлять в существующие inline harnesses `src/ui/*`; `cargo test --bin ak-configurator ui::` покрывает UI-модули. До приёмки filtered run проверить список и число тестов: ноль совпадений — не PASS. Единственные optional features — `mcp` и `mcp-mock`; native-тестам они не нужны.
10. Перед каждым commit/push получить независимое ревью кода, документов и evidence. После substantive fix повторить тесты и ревью.

## 2. Архитектурные решения, которые больше не обсуждаются

### 2.1 Минимальное расширение image contract

Не переписывать подсистему в универсальный plugin registry. Сохранить `ImageContract`, добавить `ImageContract::Beta5(ProfileId)` и направить его на новую матрицу beta.5. `contract_for_hash` вызывается только после вычисления SHA256 полного установленного CODE; совпадение должно быть однозначным.

Добавить отдельный `assets/ak820/beta5-profiles.json` и адаптер `beta5_firmware_matrix()` по образцу beta.4.1. Поля `settings_identity`, `capabilities`, `dynamic_entries`, `runtime_qmk_settings_ids`, keycode/decoder policy и packed digests берутся только из финальных pinned входов. Не наследовать их молча от beta.4.1.

### 2.2 Один выбор выпуска вместо boolean

В `InstallerRuntime` заменить `beta4_1: bool` на `selected_release: InstallerRelease`:

```rust
enum InstallerRelease { Beta3, Beta4_1, Beta5 }
```

`InstallerRelease` задаёт label, `ReleaseAdmission`, profile adapter и правило defaults. Для beta.3 и beta.4.1 поведение не менять. Beta.5 становится default UI target, но по-прежнему требует явного `prerelease_opt_in`. Не удалять beta.3 в этом этапе: его поддержка как исторического входа не равна разрешённому переходу на beta.5.

`ReleaseAdmission` должен хранить выбранный `InstallerRelease`, точный image contract и digest catalog. Проверки catalog вынести в общую функцию с явными ожидаемыми tag/digests; не добавлять третий почти одинаковый метод `beta5()`. `ReleaseObservation` должен сравнивать текущий выбранный release/catalog, а не только legacy beta.3.

### 2.3 QS v3: версия записи и default образа разделены

В `ak820_qs_versioned_offline.rs` добавить `RecordVersion::V3`, `ImageVersion::V3`, QSID 23 и десятое значение. Заголовок v3: `51 53 03 13 9f`; поле 23: offset 20, width 2, little-endian, range 1–1000. `FIELDS` остаётся одной таблицей; `field_count(V3)=10`.

Не оставлять `SettingsRecord::defaults(version)` с неявным Leader default. Изменить API на `defaults(version, leader_timeout_default)` и передавать 300 для beta.5 из exact profile contract. Для v1/v2 сохранить прежний defaults-совместимый wrapper только если он нужен существующим вызовам; v3 обязан получать default явно. V3 принимает v1/v2/v3; V2 не принимает V3; downgrade запрещён. Нормализация v1/v2→v3 добавляет QSID 23 с default текущего образа и пересчитывает CRC. Это offline model, не разрешение записи.

В connection reconciliation QSID 23 читать через `get_qmk_setting_u16`, как QSID 2/4/7/18/19/6. Не добавлять 23 в список u8 `1 | 3 | 22`.

### 2.4 Единый owner HID и конечные операции

Не создавать второй transport. Все новые/исправляемые команды идут через существующий `VialHidOperation` и `HidHelper`. UI только ставит операцию и обрабатывает результат с `generation` и request ID.

Ввести компактное представление доступности для страниц backup/status:

```text
Ready | OperationBusy | LeaseBusy | Disconnected | Failed(reason)
```

`OperationBusy` означает занятость текущей сессии, а не физическое отключение. `LeaseBusy` означает другой cooperating process. Не смешивать оба с `Disconnected`. Полная переработка всех HID errors не требуется: сначала применить модель к backup, unlock, RGB и footer.

### 2.5 Области editor-only и read-only

Beta.5 connection сначала допускает точный образ к read-only reconciliation и backup. UI записи новых Tap Dance, NKRO и Dynamic Macros не включать. Их новые keycodes появляются только в offline editor при соответствующем `keycode_source`; live UI получает capabilities только после отдельной авторизации прошивкой. Отсутствие QSID 23 в редакторе настроек на этом этапе — осознанное ограничение, а не ошибка.

## 3. Этап 0 — заморозить финальные входы beta.5

**Зависимости:** завершение firmware CI и повторная публикация release assets.
**Владелец:** release-inputs; файлы `assets/ak820/beta5-*`, `.github/scripts/test_beta5_catalog.py`, отчёт проверки.

1. Дождаться успешного CI и публикации для явно указанного commit. Runs 41294887078/41294883546 для `c9e0220c25f664c4a470faa6700059f6aedf520c` завершились failure. Последний наблюдавшийся candidate — `2ab372dab7f084b76b57aa98a9543c249a6d0c30`, runs 41297661542/41297648433 ещё in_progress на 2026-11-01; `gh release view beta.5 -R dvino/ak820` вернул `release not found`. Перед pin повторить проверку; candidate и текущий CI не доказывают успех.
2. Скачать заново опубликованные release-catalog, capability-catalog, firmware-matrix, module-descriptions и bundle по HTTPS. Зафиксировать URL, tag, дату, размеры и SHA256 всех пяти входов.
3. Записать точный firmware commit/tag и run URL в `docs/STATUS.md` и RU. Если tag указывает на более новый commit, использовать его, а не c9e0220c.
4. Проверить catalog: `settings_writes_authorized=false`, ровно ожидаемые 10 profiles, exact IHX/CODE bindings и только 200 разрешённых directed pairs beta.4/4.1→beta.5 и beta.5→beta.5. Для beta.3 и неизвестного source — deny.
5. Восстановить каждый профиль официальным pinned firmware packer в private temporary output. Его текущий CLI не имеет `--check`: `python3 "$FW/qmk-8051-ak820/util/pack_ak820.py" --ihx "$IHX" --map "$MAP" --stock-rom "$STOCK_ROM" --native-reference "$NATIVE_REFERENCE" --native-rom "$NATIVE_ROM" --out "$PRIVATE_OUT"`. IHX/map брать из final profile; нужны отдельно проверенные локальные OEM/native inputs и новый несуществующий output directory для каждого профиля. Это только файловые inputs, не device operations. Отдельно проверить IHX/map digests, затем reconstructed permanent packed CODE bytes/hash против published profile identity; IHX text не равен packed bytes. Требуется 10/10. Packer также создаёт private recovery/full-ROM files: outputs, OEM inputs и raw reports не копировать в репозиторий. Сохранить только metadata/digests, удалить лишь собственные disposable outputs после проверки, что они не используются. Missing inputs, changed CLI или mismatch блокируют gate; synthetic OEM и linked fingerprints не подставлять.
6. Заменить staged старые `beta5-*.json` финальными байтами. Обновить test так, чтобы он проверял не только внутренние ссылки, но и зафиксированные внешние digests и 200 пар. Встроенный bundle в программу не добавлять: prepared payload остаётся допустимым входом.
7. Команды:

```bash
# Выполнять из уже проверенного собственного worktree, не менять checkout.
python3 .github/scripts/test_beta5_catalog.py
```

8. **Gate:** нет финальных digests или pack check не 10/10 — не начинать этап 1. Оформить blocker пользователю, не подставлять старые значения.

## 4. Этап 1 — offline QS v3 и профили beta.5

**Зависимости:** этап 0. **Issue:** базовый контракт для #2 и всех live операций.
**Файлы:** `src/ak820_qs_versioned_offline.rs`, `src/ak820_settings_model.rs`, `src/ak820_typed_storage.rs`, `src/ak820_restore_plan.rs`, `src/ak820_settings_backup.rs`, `src/ak820.rs`, `src/ak820_beta1_profiles.rs`, новые offline tests.

1. Реализовать решения 2.1–2.3 без изменения live writer.
2. Добавить golden v3 defaults и maxima, рассчитанные независимым firmware Python reference из pinned commit; зафиксировать происхождение vectors. Проверить Leader=1,300,1000; 0,1001,65535 reject.
3. Добавить roundtrip v1/v2/v3, erased 0x00/0xFF, v1/v2→v3, запрет v3→v2/v1, CRC/header/reserved corruption и property/fuzz decode. Восстановление offline v2→v3 проверить на synthetic backup; production restore не включать.
4. Проверить 10/10 module profiles и точное соответствие matrix/capabilities. Tap Dance 8, Combo 8, Key Override 20, 6 layers, encoder и RGB limits взять из финальных файлов, не из предположений.
5. Команды:

```bash
cargo fmt --check
cargo test --bin ak-configurator ak820_qs_versioned_offline
cargo test --bin ak-configurator ak820_settings_model
cargo test --bin ak-configurator ak820_restore_plan
cargo test --bin ak-configurator beta5
cargo clippy --all-targets -- -D warnings
```

6. **Приёмка:** новые тесты зелёные; ни один production write path не стал доступен; EN/RU limitations явно называют offline-only QS migration.

## 5. Этап 2 — read-only recognition, connection и полный snapshot (#2)

**Зависимости:** этап 1. **Файлы:** `src/ui/ak820_connection.rs`, `src/ak820_settings_backup.rs`, `src/ak820_backup_capture.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/vial_hid_task.rs`, `src/ui/ak820_status.rs`, EN/RU catalogs.

1. Добавить Beta5 в `ImageContract::{matrix_profile,profile,label,rgb_modes,qsid_ids,dynamic_counts,supports_keycode}` и `contract_for_hash`. Не признавать образ по VID/PID, имени или schema без CODE digest.
2. В read path использовать `ImageVersion::V3`, record v3 и явный default. Сверить full NVM, published macro buffer, QSID list и все значения, включая u16 QSID 23. При mismatch завершить connection read-only с конкретной причиной.
3. Backup сохраняет observed source identity, selected target release, catalog digest, CODE digest, QS source/target record versions и `settings_writes_authorized=false`. Не объявлять snapshot atomic. Beta.4 и beta.4.1 имеют одинаковые IHX/map/packed CODE (`src/ak820_beta1_profiles.rs`): canonical observed identity — `ImageContract::Beta4`, явная подпись alias — `beta.4 / beta.4.1 (одинаковые байты)`. По устройству нельзя определить пакет установки. Доверенная локальная installation receipt может добавить отдельно подписанное package provenance, но не заменить observed identity. Выбранная цель не определяет происхождение source.
4. Исправить #2: при отказе lease показывать `LeaseBusy`; при активной capture/restore — `OperationBusy`; при потере handle — `Disconnected`. Capture сохраняет уже прочитанный NVM/результат, если он полностью проверен; ошибка не стирает предыдущий preview без необходимости.
5. В `Problem::Capture` перестать терять исходную ошибку: worker result должен нести локализуемую причину и безопасный technical detail. Не показывать «device disconnected» при busy writer.
6. Restore из beta.4/4.1 в beta.5 оставить заблокированным в UI до отдельного разрешения. Не интерпретировать 200 catalog pairs как restore authorization. Factory reset также не расширять.
7. Тесты: synthetic snapshots beta.4.1/beta.5; unsupported CODE; v1/v2/v3 NVM; macro getter mismatch; QS23 mismatch; lease contention; busy-during-capture; disconnect-during-capture; completion generation. Команды:

```bash
cargo test --bin ak-configurator ak820_backup
cargo test --bin ak-configurator ak820_connection
cargo test --bin ak-configurator beta5
cargo test --bin ak-configurator ui::
```

8. **Приёмка:** проверить три availability states и читаемые EN/RU causes. Добавить synthetic inspection fixtures прежней структуры `ak820-settings-backup-2` (manifest/settings/NVM/full-ROM), не копируя OEM data: checked compatible preview, когда policy разрешает, иначе конкретная image/version/policy incompatibility, не disconnected. Комментарий #2 отделяет snapshot failure от непроверенного blocked restore; restore остаётся denied. Offline repair записывается отдельно; live Windows issue закрывается только после read-only snapshot admission и specific preview/denial evidence на exact firmware. Fixtures не доказывают USB behavior.

## 6. Этап 3 — асинхронный RGB без повторов (#3)

**Зависимости:** этап 2. **Файлы:** `src/ui/rgb_settings.rs`, `src/ui/vial_hid_task.rs`, `src/hid_ak820.rs` только для диагностики, RGB UI tests.

1. Убрать прямые вызовы `hid.set_vialrgb_mode` из `set_rgb_effect`, `set_rgb_brightness`, `set_rgb_color`, `set_rgb_speed`. Добавить один `VialHidOperation::Ak820RgbWrite` с полным снимком `{effect,speed,hue,saturation,brightness}`, request ID, generation и old value.
2. До submit валидировать exact Beta5 image, writable authorization и supported effect/ranges. Хранить ровно одну submitted command и не более одного unsent coalesced draft: новое UI-изменение заменяет только unsent draft; после его submit более новое изменение может стать единственным новым draft. Не создавать бесконечную очередь, не отменять и не дублировать отправленную команду.
3. Worker выполняет ровно один `set_vialrgb_mode`, затем существующий обязательный readback там, где он уже предусмотрен. Timeout/error/readback mismatch завершает операцию; UI оставляет значение неопределённым/старым и не делает autosave повторно.
4. Отключить RGB controls на время submitted operation, но не блокировать repaint всего окна. Scene stop остаётся перед постановкой команды.
5. Тесты с fake delayed HID: 5-секундная задержка не останавливает frame progress; rapid changes создают не более одной submitted команды и одного coalesced draft; timeout не порождает retry; stale generation не меняет новую сессию.
6. Команды:

```bash
cargo test --bin ak-configurator rgb
cargo test --bin ak-configurator vial_hid_task
cargo test --bin ak-configurator ui::
```

7. **Приёмка:** headless доказывает отсутствие sync wait/retry и safe Quit/window close при delayed/failed worker completion, без новых команд на exit. Для #3 записать chosen effect, exact image/candidate, active owner, request/result или timeout и close behavior без private payloads. Windows issue #3 остаётся open до physical evidence; каждое live RGB действие требует fresh write authorization.

## 7. Этап 4 — конечный unlock (#4)

**Зависимости:** этап 3, общий worker result. **Файлы:** `src/vial/unlock.rs`, `src/ui/vial_hid_task.rs`, unlock tests, EN/RU messages.

1. Добавить deadline 15 секунд от успешного `UNLOCK_START` и состояние `TimedOut`. Poll остаётся 200 ms; repaint не управляет протокольным timeout.
2. `fail_vial_unlock_poll` больше не пишет «retrying» и не оставляет бесконечный polling: первый transport error останавливает session с конкретной ошибкой. Не отправлять повторный `UNLOCK_START` автоматически.
3. На timeout показать: отпустить клавиши, проверить подсветку/отклик, закрыть overlay и повторить только явным действием. Не выполнять reset/reconnect автоматически.
4. Разрешить отмену до start; после start кнопка «Stop waiting» прекращает только ожидание UI, не утверждает, что firmware unlock отменён. При выходе worker завершается без новых команд.
5. Тесты: already unlocked; correct hold; wrong keys; counter regress; poll error; 15-секундный virtual-clock timeout; close before/after start; generation change; message содержит «проверьте работу клавиатуры».
6. Команды: `cargo test --bin ak-configurator vial_unlock && cargo test --bin ak-configurator unlock`.
7. **Приёмка:** каждый путь terminal без automatic retry. Добавить background layout/RGB ownership contention и already-unlocked response; UI поясняет physical hold и started/completed/rejected/timed-out states. Evidence: exact candidate/image, UI step, protocol outcome. Offline boundedness не доказывает исправление live unlock: #4 open до отдельно авторизованной Windows проверки на exact image.

## 8. Этап 5 — сохранить UI при OS-indicator refresh (#5)

**Файлы:** `src/ui/ak820_os_indicator.rs`, `src/ui/device_connection.rs`, `src/ui/ak820_status.rs`, `src/ui/ak820_backup_inspection.rs`, indicator/native layout tests. Текущий код использует `Job::Sample`, `Job::Write`, `poll_ak820_os_indicator_task`, `tick_ak820_os_indicator`; API `LayoutPoll` и `poll_layout()` нет. Второй polling path не создавать.

1. Сохранить sample generation/freshness checks и HID loan cache (`ak820_os_indicator_display_connected`), tests `unchanged_layout_samples_keep_hid_and_ui_availability`, `competing_work_discards_sample_before_borrowing_hid`, `indicator_hid_loan_preserves_cached_page_shapes_without_writes`. Это прежние частичные исправления, не полное evidence закрытия #5.
2. Добавить test-only injection Sample/Write completion без реального HID I/O. OS sampling не заимствует HID. Во время verified same-generation loan показывать connected metadata из cached verified facts; временный `hid_device.take()` не равен physical disconnect. Реальный disconnect/generation change немедленно инвалидирует cache.
3. Последовательно применять существующий connected-display predicate к app-version/footer, service-page routing и Backups availability. Page/dialog IDs и capture state не зависят от sample ticks. Не открывать/закрывать/пересоздавать view из-за заимствования owner; busy приостанавливает controls, не navigation state. Менять лишь predicates/transitions, на которых падает regression; не создавать второй identity cache и не менять write authorization.
4. Metadata-only debug traces: sample/completion, loan begin/end, generation, selected page, availability reason, без settings/payloads. Inject stale-generation results и competing work; assert saved mappings/drafts не меняются, новых writes и ложного disconnect нет.
5. Добавить многофреймовый egui regression: отдельно Main и Backups, 100 samples постоянной EN, затем 100 samples чередования EN/RU. Каждый frame проверяет наличие версии приложения, selected page ID, backup inspection/capture state, modal ID/open state, connection generation, availability и owner. Версия не исчезает, страницы/диалоги не пересоздаются и не закрываются, capture сохраняется; poll сам по себе не создаёт `Disconnected` или новую generation. Меняются только detection/LED status. Повторить с operation busy и lease busy: paused, не disconnected.
6. Команды: `cargo test --bin ak-configurator ak820_os_indicator && cargo test --bin ak-configurator ui::`. Для live acceptance сравнить установленный Windows candidate run 40632342950 с исправленным candidate при exact firmware/CODE: Main и Backups, постоянная раскладка и повторные EN/RU переключения, frame/time evidence и availability transitions. Проверка read-only: не менять mappings и не включать indicator writes; если inherited helper пишет автоматически, оставить его выключенным и отметить live indicator coverage pending operation-specific authorization.
7. **Приёмка:** sample/loan unit tests доказывают лишь bounded helper behavior, не закрывают #5. Offline acceptance требует всех многофреймовых assertions; #5 остаётся open до Windows read-only наблюдения без мерцания версии/Backups, ложного disconnect и пересоздания views. Один screenshot не доказывает временную стабильность.

## 9. Этап 6 — завершить main-window интеграцию (#6)

**Файлы:** `src/ui/ak820_status.rs`, `src/ui/settings_shell.rs`, `src/ui/layout_settings_dropdown.rs`, `src/ui/app_settings.rs`, `src/ui/ak820_backup_inspection.rs`, `src/ui/ak820_offline_extensions.rs`, `src/ui/ak820_install_files.rs`, `src/ui/ak820_install.rs`, EN/RU catalogs/docs. Сохранить native pages. Матрица ниже — готовый контракт исполнения, не поручение заново проектировать навигацию.

| Требование issue | Что выполнено / пробел | Конкретное изменение | Проверка |
|---|---|---|---|
| Постоянные firmware/install, backup/restore, app settings pages | Native routing есть в `ak820_status.rs`; installer/files/backups интегрированы `936a989`; `settings_shell.rs::open_app_settings_page` открывает AppSettings. | Сохранить routes/state; убрать только оставшиеся modal-only duplicate entry points. | Каждый пункт меню открывает main-window page со стабильными ID/state при Back/reopen, keyboard focus, без нового top-level window. |
| Выбранное устройство и проверенные source/target | Есть source ROM SHA и target profile; Beta4 recognition не различает byte-identical пакеты beta.4/4.1. | Постоянные строки `Подключённое устройство / Исходная прошивка` и `Выбранная цель установки`: canonical observed image/profile/CODE, alias beta.4/4.1 и отдельно receipt provenance при наличии. Не выводить source package из target. Unknown source блокирует install. Beta.5 default по stage 8; beta.3 явно alternate только для supported pairs. | Одинаковые beta.4/4.1 bytes дают одну observed identity и alias; receipt меняет только provenance; beta.5 target не меняет source; supported beta.3 alternate; unknown denial. |
| Первый шаг restore и safety | Capture/inspection есть; restore блокируется release policy. | Над backup actions: `1. Открыть существующую копию → 2. Проверить источник и совместимость → 3. Просмотреть условия восстановления`. Для beta.5: `Восстановление недоступно: запись сохранённых настроек не разрешена для этого выпуска`; inspection доступен. Не добавлять live restore shortcut. | Existing/corrupt backup, no device, incompatible source, blocked beta.5 restore; inspection не вызывает writer/loader. |
| Prerequisites/consequences, Back/Cancel, blocked reasons | Busy/confirmation есть; stage 2 исправляет domain causes. | Использовать reasons stage 2 и preflight summary; Back до submit, точная stop semantics после, без обещания rollback. | Ready/busy/lease/disconnected/unverified; cancel до/после submit, Tab/Shift-Tab, repaint. |
| Дублирование Application settings | Native duplicate уже устранён: `src/ui/ak820_status.rs` содержит постоянную service-navigation кнопку AppSettings. Settings-dropdown entry и sizing label в `src/ui/layout_settings_dropdown.rs` — Wasm-only; неиспользуемый match arm не означает видимый пункт меню. | Сохранить native service navigation и Wasm-only dropdown; не удалять их и не создавать вымышленный application-menu entry. Новый код удаления native duplicate не нужен. | Отрисовать native EN/RU navigation и проверить один предусмотренный AppSettings route к той же постоянной странице; контекстные поясняющие ссылки не считать дубликатами меню. |
| Непонятные русские подписи | `app_settings.rs` использует `ui.middle_click_transparent_label`/tooltip и app import/export keys; literal legacy wording здесь уже нет, улучшения сохранить. | RU: `Средняя кнопка мыши: назначать прозрачную клавишу`; tooltip — назначение `KC_TRNS` в редакторе, не прозрачность клавиатуры/LED. `Импорт настроек приложения` / `Экспорт настроек приложения`: local app preferences, не AK820 firmware backup/restore. EN имеет тот же scope; formats не менять. | Обе локали; нет `Middle click → Transparent` / `Import/Export Entropy` и намёка на firmware writes; import/export roundtrip не изменён. |

1. Выполнить только изменения матрицы, по regression на строку; сохранить close/reopen/stale-capture tests.
2. Выполнить `cargo test --bin ak-configurator ui::`, включая affected menu/settings/backup regressions; open→close→capture→cancel→reread→exit без disconnect loops.
3. Приложить commit/test evidence по строкам. Закрыть #6 только после всех строк; RGB/unlock fixes отдельными коммитами.

## 10. Этап 7 — разделить OS detection и LED mapping (#7)

**Файлы:** `src/ui/ak820_os_indicator.rs` (settings rows, `status_label`, `mode_label`, tests), `src/ui/app_settings.rs`, EN/RU text. Issue не о геометрии клавиатуры, encoder order или split layout. Fixing commit не установлен; не приписывать исправление `936a989`.

1. Сохранить частичные улучшения: rows уже говорят `LED for English/Russian layout`, `status_label` различает detected/paused, EN modes — `LED on/off`. Дополнить объяснение, не переписывать helper.
2. Status: `Обнаруженная раскладка ОС` / `Detected OS layout`. Choices: `Поведение LED при английской раскладке` / `LED behavior when English is detected` и аналогично для русской. Постоянное пояснение: `Обнаружение показывает текущую раскладку ОС. Варианты ниже задают поведение LED, а не активность раскладки.` EN эквивалент. Normal = обычное поведение firmware, On = LED горит, Off = не горит, Breathing = медленное дыхание; RU `LED включён/выключен`, не голое `Вкл./Выкл.`.
3. Busy/unverified: detection paused, текущая раскладка не обновлена. Последнее значение, если показано, явно last observed, не новая detection. Pause/resume не меняет saved EN/RU choices, generation или page. Disabled/unavailable/error отдельны; без automatic reset/retry/write.
4. Тесты: detected EN при EN=Off/RU=On показывает EN detection и оба неизменённых mapping без противоречия; RU меняет лишь detection; busy/unverified pause/resume сохраняют choices; unavailable/disabled/error не выдумывают detection. Все mode explanations EN/RU, persistent widget IDs, rendering/status update не сохраняет config и не пишет HID.
5. `cargo test --bin ak-configurator ak820_os_indicator` и `cargo test --bin ak-configurator ui::`. На Windows read-only проверить exact connected firmware/profile/CODE: detected layout, saved mappings и physical LED observation, без изменения indicator options и write commands. Если helper будет писать, оставить выключенным и physical mapping confirmation отметить pending separate authorization. Историческая beta.4 observation не доказывает beta.5.
6. Закрыть #7 только с wording/state tests и exact-firmware read-only mapping/LED evidence. Иначе `UI clarification complete; firmware/LED observation pending`, issue open. Тесты 65/75/100 или encoder не заменяют эти критерии.

## 11. Этап 8 — beta.5 installer без write authorization

**Зависимости:** этапы 0–2. **Файлы:** `src/ak820_release.rs`, `src/ui/ak820_install.rs`, `src/ak820_install_executor.rs`, installer tests, `docs/INSTALL*`, help EN/RU.

1. Ввести `InstallerRelease`, сделать Beta5 default, сохранить explicit prerelease checkbox и existing confirmation token/3-second Esc+left Ctrl. Typed text, Vial unlock или manual Bootloader не заменяют подтверждение.
2. Обновить `ReleaseAdmission` по 2.2. Проверять final catalog digest, matrix/capability digests, selected profile IHX/CODE и source/target pair. Unknown identity deny.
3. При source beta.4/4.1 требовать verified source CODE; beta.5 reinstall требует beta.5 CODE; beta.3 deny до erase. Не читать полный CODE только из catalog fingerprint.
4. Все подготовленные payload и exact confirmation contents формируются до erase. Flash один раз, verify, bounded terminal states; никаких blind retries/reset/restore.
5. Тесты admission для 200 allow pairs и representative deny: beta.3→beta.5, unknown→beta.5, beta.5→beta.4.1, digest mismatch, prerelease off, catalog substitution. Не исполнять physical flash в автоматическом CI.
6. Команды: `cargo test --bin ak-configurator ak820_release && cargo test --bin ak-configurator ak820_install`.
7. **Приёмка:** offline install plan/admission готов. Physical flashing остаётся отдельно авторизуемой операцией и не нужен для завершения этого плана.

## 12. Этап 9 — регрессии, документация и выпуск companion

**Зависимости:** этапы 0–8. **Файлы:** `README*`, `docs/INSTALL*`, `docs/STATUS*`, `docs/REQUIREMENTS*`, release notes, help/i18n.

1. Обновить default target beta.5, предыдущий beta.4/4.1 compatibility, read-only QS23, отсутствие live writes новых modules и ограничения hardware evidence. Удалить утверждения о старых beta.5 assets как текущих.
2. Выполнить полный offline набор после всех изменений один раз:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --bin ak-configurator # включает native/headless UI tests
python3 .github/scripts/test_beta5_catalog.py
```

Проверить ссылки изменённых Markdown-документов в ревью; отдельного repository doc-link checker сейчас нет.
3. Зафиксировать число tests и окружение. Пропущенные Windows/macOS/physical проверки перечислить, не называть их pass.
4. Сопоставить каждое issue #2–#7 с commit/test/evidence и закрыть только выполненное. #3 без Windows hardware остаётся open; #2 и #4 остаются open до соответствующего live evidence; offline software milestone не равен закрытию исходного hardware report.
5. Не собирать companion release, пока независимое ревью не закрыло findings. Release targets и SHA256 — отдельная release задача, не часть текущего плана.

## 13. Порядок коммитов и независимое ревью

Рекомендуемые атомарные коммиты строго в порядке этапов:

1. `ak820: pin final beta.5 release inputs`
2. `ak820: add beta.5 QS v3 offline contracts`
3. `ak820: recognize beta.5 and repair backup availability`
4. `ak820: move RGB writes to bounded HID worker`
5. `ak820: make unlock polling finite`
6. `ak820: preserve service pages during indicator refresh`
7. `ui: finish native service-page integration`
8. `ui: clarify OS detection and LED mapping states`
9. `ak820: admit exact beta.5 installer transitions`
10. `docs: record beta.5 verification and limitations`

Для каждого: автор не утверждает собственное изменение; reviewer получает diff, команды, результаты и открытые ограничения. Findings записываются в `docs/audits/`; исправление требует повторного теста и нового ревью. Не делать push без отдельного разрешения пользователя.

## 14. Что исполнитель не должен решать самостоятельно

- Не выбирать другой firmware commit или digest при failed CI.
- Не разрешать settings writes, restore, reset или flash по catalog flags.
- Не добавлять browser/Wasm client до usable desktop milestone.
- Не переносить OEM/firmware tree и не менять backup format без необходимости.
- Не расширять задачу до универсального HID framework, registry plugins или массового refactor.
- Не закрывать hardware issues по offline tests.
- При отсутствии финальных release assets остановиться на этапе 0 и запросить входы; остальные этапы не выполнять на superseded data.
