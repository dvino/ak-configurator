# Q02 — срок жизни Windows helper

English: [EN](../../session-evidence/Q02-HELPER-LIFETIME.md).

## Текущие результаты

**Исправление срока жизни helper прошло source review и целевые native-host tests. Ограниченная офлайн-область C04 для Windows принята; B12 интегрируется, а Q02 в целом ожидает остальных проверок срока жизни процесса и платформ.** Windows proxy теперь создаёт unnamed, non-inheritable Job Object с `KILL_ON_JOB_CLOSE` и хранит его в существующем slot. Child должен быть назначен в job до завершения stdin startup barrier, поэтому до назначения не может открыть HID. Ошибка назначения приводит к fail-closed. Ограничения nested jobs остаются совместимостным пределом.

Reviewer `helper_lifetime_review` выявил риск PID reuse в test fixture до запуска parent-death probe. Fixture заменён на дублирование действительного child process handle из сохранённого живого parent. Reviewer закрыл finding; process IDs не записываются.

| Проверка | Результат |
|---|---|
| Startup barrier: `rtk cargo test --locked --bin ak-configurator hid::hid_proxy::tests::windows_helper_start_barrier_requires_complete_token -- --exact --nocapture` | 1 passed, 1961 filtered, 0,00 s, exit 0 |
| Parent-death fake-child test на текущем test binary | 1 passed, 1961 filtered, 0,08 s, exit 0 |
| Полная группа `hid::hid_proxy::tests` на том же binary | 32 passed, 3 ignored, 1927 filtered, 7,96 s, exit 0; включены blocked read/write timeout и reaper tests при живом parent; ignored не квалифицированы |
| Marker-based завершение parent | Fake child принимает startup marker и затем безусловно ждёт 60 секунд. При обычном `process::exit` и принудительном завершении parent child выходил в пределах 5 секунд; факт проверялся ожиданием на сохранённом process handle |
| Среда сборки/тестов | Native Windows/MSVC; изолированный `CARGO_TARGET_DIR=D:\codex-ak-configurator-c02-target`; отключены debug info DEV/TEST и incremental compilation; 2 Cargo jobs; `RUSTFLAGS=-C debuginfo=0` |
| Проверенные source и executable | В тестовом snapshot `src/hid_proxy.rs` SHA256 `66f04c275f53660add0b822f5ee67262c1112800b42aebf24636b6813c82e9f2`; test executable 99 862 528 bytes, SHA256 `67bef0ac18a014cacc091afa5792b7838499aace65920d27997fd43d76a71d3d` |
| Форматирование | Первая команда `rustfmt --edition 2021 --check src/hid_proxy.rs` выявила diff форматирования. Применён `rustfmt --edition 2021 src/hid_proxy.rs`, после чего per-file check прошёл (exit 0). SHA256 текущего отформатированного файла `70a11ad321e63faad8e2cda04f840dab835bbd322e481738efb49826d29c995d`; тесты выше запускались на pre-format source snapshot |
| Форматирование и строгие lint интегрированного snapshot | После включения в общий 71-file B08-C03 source checkpoint полные `cargo fmt --check` и строгие all-target `LINT-N`/`LINT-C` прошли (exit 0). Production-код helper не менялся. Целевые helper-тесты выше остаются историческими и на этом snapshot повторно не запускались |

Предыдущая exact-filter команда была запущена на устаревшем executable и нашла 0 тестов (1958 filtered); это не pass. Учитывается исправленный direct run на текущем test binary. Ранее проведённый 250 ms no-HID probe сохранён только как исторический: он не доказывал, что child вошёл в blocked I/O.

Эти тесты проверяют намеренно задержанный fake child и cleanup process handle. Они не доказывают завершение при блокировке HID syscall или при обычном выходе GUI и не квалифицируют native window, физическое устройство, macOS или всю платформенную/артефактную часть Q02. HID packets и взаимодействия с устройством не было. Ограниченная офлайн-область C04 принята; идёт интеграция B12, а Q02 ожидает оставшихся платформенных проверок и тестов срока жизни процесса. User-specific paths и process IDs не включены.

Проверенный source snapshot и executable сохранены вне repository в private handoff checkpoint. Более поздние formatting/lint интегрированного snapshot не подтверждают обычный выход GUI, завершение при blocked HID syscall, завершение Q02 или поведение устройства.
