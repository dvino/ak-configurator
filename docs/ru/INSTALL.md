English source of truth: [Installation and development](../INSTALL.md).

# Установка и разработка

Первый этап — нативный десктопный конфигуратор. Текущие результаты сборки и
тестов приведены в [статусе](STATUS.md). Браузерная версия последует после
десктопного этапа.

## Сборка из исходников

Установите Rust 1.97 и системные зависимости для сборки. В Ubuntu 24.04
установите зависимости нативной сборки Linux, используемые в workflow
`desktop-dev`:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config libdbus-1-dev libgtk-3-dev libudev-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev zenity
```

В Linux диалоги Save/Open используют доступный XDG desktop portal или
резервный `zenity`. В минимальном окружении рабочего стола или контейнере могут
отсутствовать оба варианта; перед работой с файлами установите `zenity` или
реализацию desktop portal для текущего сеанса.

Из корня репозитория соберите исполняемый файл и запустите тесты на хосте:

```sh
cargo build --release --locked
cargo test --locked --all-targets
```

Для офлайн-предпросмотра интерфейса с примерными данными AK820 выполните:

```sh
cargo run --locked -- --demo-ak820
```

Демонстрационный режим не ищет HID-устройства и не открывает их. Пример
раскладки не является копией настроек физической клавиатуры. Исполняемый файл
релизной сборки находится по пути `target/release/ak-configurator` в Linux и
macOS или `target/release/ak-configurator.exe` в Windows. Сборка и тесты на
хосте не подтверждают работу с физической клавиатурой.


Linux-файлы из CI собраны в Ubuntu 24.04. Проверенный файл x86_64 требует
glibc 2.39; его запуск в Debian 12 завершился отказом из-за отсутствия
GLIBC_2.38/2.39. Для этого
окружения предоставляется отдельная пробная сборка Debian 12 x86_64, которая
там собрана и запущена. Ни одна из них не является нативной сборкой Alpine/musl.
Обе требуют графических библиотек и portal или Zenity для файловых диалогов.

## Доступ к устройству в Linux

Если Linux запрещает доступ к HID-интерфейсу клавиатуры, явно установите
правило udev из корня репозитория:

```sh
sh linux/udev/install-vial-rules.sh
```

При необходимости скрипт запросит повышение прав через `pkexec` или `sudo` и
запишет `/etc/udev/rules.d/59-ak820.rules`. Правило даёт доступ только USB
идентификаторам AK820. Это необязательное изменение системы; сборка и запуск
демонстрационного режима его не выполняют.

## Поддерживаемое устройство и пакеты

Эта альфа-версия принимает только проводную AK820 с проверенным полным
постоянным образом v0.6.0-alpha.3. Проверенная область ROM: 0x2800–0xC700,
SHA-256: `01f0f7a979d8cfa8217b3bba333758ae3509cf7f3f8e35fbffb1ed034b808e5d`,
манифест находится по адресу 0xEF80. Неизвестные, пользовательские и пробные
образы отклоняются. Перед настройкой прочитайте [руководство](DESKTOP.md).

Workflow `desktop-dev` описывает исполняемые файлы разработки для Linux и
Windows x86_64/arm64, а также macOS arm64/x86_64. После успешного запуска
workflow шесть артефактов будут доступны на странице [запусков desktop-dev в
Actions](https://github.com/dvino/ak-configurator/actions/workflows/desktop-dev.yml?query=branch%3Adesktop-dev);
каждый содержит исполняемый файл и `SHA256SUMS`. Перед загрузкой проверьте,
что задание для нужной платформы прошло. Это неподписанные сборки для
разработки, а не установщики. С параметром `--demo-ak820` профили beta.1 открывают редактируемые локальные
проекты примеров с сохранением, открытием и отменой импорта. Вне деморежима
каталог остаётся предварительным просмотром; запись настроек beta-прошивки
и её установка отключены. Приложение распознаёт только точный образ alpha.3. Восстановление
OEM и системный помощник LED не входят в этот этап десктопной версии.

Выберите артефакт по операционной системе и процессору:
`development-binary-linux-x86_64`, `development-binary-linux-arm64`,
`development-binary-windows-x86_64`, `development-binary-windows-arm64`,
`development-binary-macos-arm64` или `development-binary-macos-x86_64`.
Скачайте ZIP выбранного артефакта из успешного запуска workflow и распакуйте
его. Перед запуском сверьте исполняемый файл с `SHA256SUMS`. В Linux выполните
`sha256sum -c SHA256SUMS`, затем `chmod +x ak-configurator &&
./ak-configurator`. В macOS выполните `shasum -a 256 -c SHA256SUMS`, затем
`chmod +x ak-configurator` и `./ak-configurator`. В Windows PowerShell
сравните результат команды
`(Get-FileHash .\ak-configurator.exe -Algorithm SHA256).Hash.ToLowerInvariant()`
с хешем в файле `SHA256SUMS`, затем запустите
`Start-Process .\ak-configurator.exe`.
