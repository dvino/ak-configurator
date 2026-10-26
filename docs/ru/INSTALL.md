English source of truth: [Installation and development](../INSTALL.md).

# Установка и разработка

AK Configurator v0.1.0-beta.2 находится на этапе подготовки выпуска. Текущие проверки исходников и ограничения приведены в [статусе](STATUS.md) и [отчёте](REVIEW.md). Workflow шести платформ задаёт сборки для разработки и отдельный неподписанный кандидат DRAFT; публикация кандидата не выполнялась. Изолированные typecheck адаптера Windows GNU и macOS arm64 не являются полной сборкой или квалификацией запуска. Браузерная версия последует после нативного этапа.

## Сборка из исходников

Используйте Rust 1.97 и системные зависимости выбранной платформы. В Ubuntu 24.04 установите зависимости Linux, используемые в `desktop-dev`:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libdbus-1-dev libgtk-3-dev libudev-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev zenity
```

Диалоги Save/Open в Linux используют доступный XDG desktop portal или резервный `zenity`. В минимальном окружении рабочего стола или контейнере может не быть ни того, ни другого; установите один из вариантов перед работой с файлами.

Из корня репозитория соберите приложение и последовательно запустите тесты:

```sh
cargo build --release --locked
cargo test --locked --all-targets -- --test-threads=1
```

Последовательный запуск тестов снижает нестабильность HID-тестов процессов, использующих общий ресурс. Офлайн-предпросмотр интерфейса:

```sh
cargo run --locked -- --demo-ak820
```

Демонстрационный режим не ищет и не открывает HID-устройства. Примеры не являются копиями настроек устройства. Исполняемый файл находится в `target/release/ak-configurator` для Linux/macOS и `target/release/ak-configurator.exe` для Windows. Сборка и host-тесты не подтверждают физическую работу клавиатуры.

Текущие доказательства сборки исходников относятся к Ubuntu 24.04 и Rust 1.97.0. Успешный запуск старого артефакта в Debian 12 не подтверждает совместимость текущей сборки. Alpine/musl не квалифицирован. Для Linux нужны системные графические библиотеки и portal или Zenity для диалогов файлов.

## Доступ к устройству в Linux

Если Linux запрещает доступ к HID, явно установите правило udev AK820 из корня репозитория:

```sh
sh linux/udev/install-vial-rules.sh
```

При необходимости скрипт запросит повышение прав через `pkexec` или `sudo` и запишет `/etc/udev/rules.d/59-ak820.rules`. Правило ограничено USB-идентификаторами AK820. Это необязательное изменение системы; сборка и демонстрационный режим его не выполняют.

## Поддерживаемое устройство и пакеты

Контракт beta2 в исходниках поддерживает десять точных упакованных образов профилей v0.6.0-beta.2: Main, Game, Mmo, Typing, Code, Pointer, Onehand, Creative, Shortcuts и Multios. Перед разрешением редактирования приложение проверяет идентичность упакованных CODE/настроек и обязательные настройки. Неизвестные, пользовательские, пробные и несовпадающие образы отклоняются. См. [руководство по десктопной версии](DESKTOP.md). Окно «Файлы прошивки» сейчас только проверяет локальные байты и не прошивает устройство. Отдельное нативное окно установки реализует точный допуск beta2, переход в загрузчик/переподключение по порту, долговечные защитные данные, однократную транзакцию и проверку полного ROM с раздельными результатами прошивки/настроек. Первая установка встроенной прошивки с точно проверенной OEM может локально использовать свежий снимок без заранее сохранённого ROM-файла при явных настройках по умолчанию; обновления проекта и подготовленные payload требуют поддерживаемый личный OEM reference. Неверные выбранные reference не заменяются fallback. Обязательные защитные данные и записанное доверие к манифесту восстановления синхронизируются/перечитываются до стирания; ошибка останавливает поток с сохранением записанных путей. Это реализация исходников: физическая прошивка, сохранение и возврат OEM не квалифицированы.

Workflow `desktop-dev` задаёт исполняемые файлы разработки для Linux и Windows x86_64/arm64 и macOS x86_64/arm64. Каждый артефакт содержит исполняемый файл и `SHA256SUMS`; это неподписанные сборки разработки, а не установщики. Для загрузки нужен успешный запуск workflow на текущих исходниках. Известный успешный запуск шести платформ на коммите `1afd0f1` исторический и не подтверждает исходники beta2.

Выберите подходящий артефакт: `development-binary-linux-x86_64`, `development-binary-linux-arm64`, `development-binary-windows-x86_64`, `development-binary-windows-arm64`, `development-binary-macos-arm64` или `development-binary-macos-x86_64`. Скачайте ZIP из успешного текущего запуска и распакуйте. Перед запуском сверьте исполняемый файл с `SHA256SUMS`. В Linux выполните `sha256sum -c SHA256SUMS`, затем `chmod +x ak-configurator && ./ak-configurator`. В macOS выполните `shasum -a 256 -c SHA256SUMS`, затем `chmod +x ak-configurator` и `./ak-configurator`. В Windows PowerShell сравните `(Get-FileHash .\ak-configurator.exe -Algorithm SHA256).Hash.ToLowerInvariant()` со строкой в `SHA256SUMS`, затем запустите `Start-Process .\ak-configurator.exe`.

## Пакеты-кандидаты для десктопа

Запуск `desktop-dev` также загружает отдельный артефакт `desktop-candidate-${platform}` для каждой из шести платформ. Для Linux это архив `.tar.gz`, для macOS — архив приложения `.app.zip`, для Windows — переносимый `.zip`. Рядом находится файл `.sha256`. Архив содержит локализованные руководства по установке и RGB, лицензии приложения, шрифтов, иконок и eframe, сведения о происхождении прошивки, точные офлайн-каталог beta2 и публичные IHX/map-файлы профилей, манифест исходников/бинарника и внутренний `SHA256SUMS`. Текст справки также встроен в исполняемый файл. Это неподписанные пробные пакеты; пакет Windows — переносимый архив, а не установщик.

Для локальной упаковки используется только стандартная библиотека Python 3.11 или новее:

```sh
python3 scripts/package_ak820_candidate.py \
  --binary target/release/ak-configurator \
  --platform linux-x86_64 \
  --output-dir target/candidate
```

Укажите путь к бинарнику и значение `--platform` нужной цели: `linux-x86_64`, `linux-arm64`, `windows-x86_64`, `windows-arm64`, `macos-x86_64` или `macos-arm64`. Скрипт проверяет формат/архитектуру исполняемого файла и сверяет включённые публичные файлы прошивки с манифестом выпуска. По умолчанию дерево Git должно быть чистым. `--allow-dirty` предназначен только для явно обозначенного локального пробного пакета; манифест отмечает незакоммиченное состояние исходников. Перед распаковкой проверьте соседний файл контрольной суммы (`sha256sum -c <архив>.sha256` в Linux; `shasum -a 256 -c <архив>.sha256` в macOS). Затем проверьте внутренний `SHA256SUMS`. Linux: распакуйте `.tar.gz`, при необходимости разрешите запуск `ak-configurator` и запустите его. macOS: распакуйте `.app.zip` и откройте `AK-Configurator.app`; неподписанная сборка может быть заблокирована Gatekeeper и не нотарифицирована. Windows: распакуйте переносимый ZIP и запустите `ak-configurator.exe`; установщик не входит в пакет. Кандидаты — неподписанные пробные сборки; проверка архитектуры и хешей не подтверждает работу платформы или физической клавиатуры.

### Распаковка, проверка и запуск кандидата

Примеры предполагают, что архив и соседний `.sha256` уже получены из успешного текущего workflow или созданы локальным упаковщиком. Используется точное имя выбранной ОС/архитектуры; опубликованная загрузка кандидата не подразумевается. При несовпадении любой контрольной суммы остановитесь.

Linux x86_64:

```sh
set -e
archive=ak-configurator-0.1.0-beta.2-linux-x86_64.tar.gz
sha256sum -c "${archive}.sha256"
tar -xzf "$archive"
cd "${archive%.tar.gz}"
sha256sum -c SHA256SUMS
./ak-configurator
```

macOS arm64:

```sh
set -e
archive=ak-configurator-0.1.0-beta.2-macos-arm64.app.zip
shasum -a 256 -c "${archive}.sha256"
ditto -x -k "$archive" .
cd "${archive%.app.zip}"
shasum -a 256 -c SHA256SUMS
open AK-Configurator.app
```

Windows x86_64, PowerShell:

```powershell
$ErrorActionPreference = 'Stop'
$archive = 'ak-configurator-0.1.0-beta.2-windows-x86_64.zip'
$expectedHash = ((Get-Content -LiteralPath "$archive.sha256") -split '\s+', 2)[0]
$actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) { throw 'Archive SHA256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath .
Set-Location ($archive -replace '\.zip$', '')
Get-Content -LiteralPath SHA256SUMS | ForEach-Object {
    $expectedHash, $relativeFile = $_ -split '  ', 2
    $actualHash = (Get-FileHash -LiteralPath $relativeFile -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) { throw "SHA256 mismatch: $relativeFile" }
}
Start-Process .\ak-configurator.exe
```

### Запуск помощника раскладки OS

Сначала включите **Индикатор раскладки OS** и сохраните сопоставления EN/RU в приложении. Помощник включается явно; установка или распаковка пакета сама по себе не запускает HID-индикацию. Явный фоновый запуск из распакованной директории:

| Платформа | Команда |
| --- | --- |
| Linux | `./ak-configurator --ak820-helper` |
| Приложение macOS | `./AK-Configurator.app/Contents/MacOS/ak-configurator --ak820-helper` |
| Windows PowerShell | `.\ak-configurator.exe --ak820-helper` |

`--minimized`/автозапуск при явном включении также входят в helper. **Закрыть интерфейс, оставить помощник** или обычное закрытие при включённом помощнике освобождают GUI и оставляют малый цикл; повторное открытие приложения создаёт интерфейс в том же процессе. Явный выход завершает приложение. Неизвестная раскладка снимает индикацию; ошибка устройства останавливает обновления без слепого повтора. Помощник проверяет точную прошивку и уступает владельца настройкам/копиям/восстановлению/установке. Физические тайминги раскладки/LED, передача управления GUI и ресурсы платформ требуют отдельной квалификации.
