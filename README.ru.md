# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-red)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## Кабина пилота ИИ для OpenCode

EmbeddedCowork превращает OpenCode из терминального инструмента в **премиальное рабочее пространство** — созданное для разработчиков, которые проводят часы в сессиях ИИ-кодинга и нуждаются в контроле, скорости и ясности.

> OpenCode даёт вам двигатель. EmbeddedCowork даёт вам кабину.

![Многоэкземплярное рабочее пространство](docs/screenshots/newSession.png)

---

## Возможности

- **🚀 Многоэкземплярное рабочее пространство**
- **🌐 Удалённый доступ**
- **🧠 Управление сессиями**
- **🎙️ Голосовой ввод и речь**
- **🌳 Git Worktrees**
- **💬 Богатый интерфейс сообщений**
- **🧩 SideCars**
- **⌨️ Палитра команд**
- **📁 Обозреватель файловой системы**
- **🔐 Аутентификация и безопасность**
- **🔔 Уведомления**
- **🎨 Темы оформления**
- **🌍 Интернационализация**

---

## Начало работы

### 🖥️ Десктопное приложение

Доступно в сборках Electron и Tauri — выбирайте по своему предпочтению.

Загрузите последний установщик для вашей платформы из [Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases).

| Платформа | Форматы |
|-----------|---------|
| macOS | DMG, ZIP (Универсальный: Intel + Apple Silicon) |
| Windows | Установщик NSIS, ZIP (x64, ARM64) |
| Linux | AppImage, deb, tar.gz (x64, ARM64) |

### 💻 Сервер EmbeddedCowork

Запустите как локальный сервер и получите доступ через браузер. Идеально для удалённой разработки.

```bash
npx @vividcodeai/embeddedcowork --launch
```

См. [Документацию сервера](packages/server/README.md) по флагам, TLS, аутентификации и удалённому доступу.

### 🧪 Версии для разработчиков

Новейшие сборки из ветки `dev`:

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars позволяют открывать локальные веб-инструменты внутри EmbeddedCowork в виде вкладок.

<details>
<summary><strong>Конфигурация</strong></summary>

- **Имя**: Отображаемое имя в EmbeddedCowork
- **Порт**: Локальный HTTP или HTTPS сервис, работающий на `127.0.0.1:<port>`
- **Базовый путь**: Монтируется по адресу `/sidecars/:id`
- **Режим префикса**:
  - **Сохранять префикс** передаёт полный путь `/sidecars/:id/...` вышестоящему сервису
  - **Удалять префикс** удаляет `/sidecars/:id` перед передачей запроса вышестоящему сервису

</details>

<details>
<summary><strong>VSCode (OpenVSCode Server)</strong></summary>

Запуск с Docker:

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

Добавить SideCar как:

- **Имя**: `VSCode`
- **Порт**: `http://127.0.0.1:8000`
- **Базовый путь**: `/sidecars/vscode`
- **Режим префикса**: `Сохранять префикс`

</details>

<details>
<summary><strong>Терминал (ttyd)</strong></summary>

Запуск с:

```bash
ttyd --writable zsh
```

Добавить SideCar как:

- **Имя**: `Terminal`
- **Порт**: `http://127.0.0.1:7681`
- **Базовый путь**: `/sidecars/terminal`
- **Режим префикса**: `Удалять префикс`

</details>

---

## Требования

- **[OpenCode CLI](https://opencode.ai)** — должен быть установлен и находиться в `PATH`
- **Node.js 18+** — для режима сервера или сборки из исходного кода

---

## Разработка

EmbeddedCowork — это монорепозиторий, построенный на:

| Пакет | Описание |
|-------|----------|
| **[packages/server](packages/server/README.md)** | Базовая логика и CLI — рабочие пространства, прокси OpenCode, API, аутентификация, речь |
| **[packages/ui](packages/ui/README.md)** | Frontend на SolidJS — реактивный, быстрый, красивый |
| **[packages/electron-app](packages/electron-app/README.md)** | Десктопная оболочка — управление процессами, IPC, нативные диалоги |
| **[packages/tauri-app](packages/tauri-app)** | Десктопная оболочка Tauri (экспериментально) |

### Быстрый старт

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## Устранение неполадок

<details>
<summary><strong>macOS: "EmbeddedCowork.app повреждён и не может быть открыт"</strong></summary>

Флаг Gatekeeper из-за отсутствия нотаризации. Очистите атрибут карантина:

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

На Intel Mac также проверьте **Системные настройки → Конфиденциальность и безопасность** при первом запуске.
</details>

<details>
<summary><strong>Linux (Wayland + NVIDIA): Приложение Tauri сразу закрывается</strong></summary>

Проблема WebKitGTK DMA-BUF/GBM. Запустите с:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

Полное решение см. в оригинальном README.
</details>

---

## Сообщество

[![История звёзд](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## Благодарности

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**Сделано с ♥ командой [VividCodeAI](https://github.com/vividcode-ai)** · [Лицензия MIT](LICENSE)
