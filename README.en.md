# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-red)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## The AI Coding Cockpit for OpenCode

EmbeddedCowork transforms OpenCode from a terminal tool into a **premium desktop workspace** — built for developers who live inside AI coding sessions for hours and need control, speed, and clarity.

> OpenCode gives you the engine. EmbeddedCowork gives you the cockpit.

![Multi-instance workspace](docs/screenshots/newSession.png)

---

## Features

- **🚀 Multi-Instance Workspace**
- **🌐 Remote Access**
- **🧠 Session Management**
- **🎙️ Voice Input & Speech**
- **🌳 Git Worktrees**
- **💬 Rich Message Experience**
- **🧩 SideCars**
- **⌨️ Command Palette**
- **📁 File System Browser**
- **🔐 Authentication & Security**
- **🔔 Notifications**
- **🎨 Theming**
- **🌍 Internationalization**

---

## Getting Started

### 🖥️ Desktop App

Available as both Electron and Tauri builds — choose based on your preference.

Download the latest installer for your platform from [Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases).

| Platform | Formats |
|----------|---------|
| macOS | DMG, ZIP (Universal: Intel + Apple Silicon) |
| Windows | NSIS Installer, ZIP (x64, ARM64) |
| Linux | AppImage, deb, tar.gz (x64, ARM64) |

### 💻 EmbeddedCowork Server

Run as a local server and access via browser. Perfect for remote development.

```bash
npx @vividcodeai/embeddedcowork --launch
```

See [Server Documentation](packages/server/README.md) for flags, TLS, auth, and remote access.

### 🧪 Dev Releases

Bleeding-edge builds from the `dev` branch:

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars let you open local web tools inside EmbeddedCowork as tabs.

<details>
<summary><strong>Configuration</strong></summary>

- **Name**: Display name used in EmbeddedCowork
- **Port**: Local HTTP or HTTPS service running on `127.0.0.1:<port>`
- **Base path**: Mounted under `/sidecars/:id`
- **Prefix mode**:
  - **Preserve prefix** forwards the full `/sidecars/:id/...` path upstream
  - **Strip prefix** removes `/sidecars/:id` before forwarding the request upstream

</details>

<details>
<summary><strong>VSCode (OpenVSCode Server)</strong></summary>

Run with Docker:

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

Add SideCar as:

- **Name**: `VSCode`
- **Port**: `http://127.0.0.1:8000`
- **Base path**: `/sidecars/vscode`
- **Prefix mode**: `Preserve prefix`

</details>

<details>
<summary><strong>Terminal (ttyd)</strong></summary>

Run with:

```bash
ttyd --writable zsh
```

Add SideCar as:

- **Name**: `Terminal`
- **Port**: `http://127.0.0.1:7681`
- **Base path**: `/sidecars/terminal`
- **Prefix mode**: `Strip prefix`

</details>

---

## Requirements

- **[OpenCode CLI](https://opencode.ai)** — must be installed and in your `PATH`
- **Node.js 18+** — for server mode or building from source

---

## Development

EmbeddedCowork is a monorepo built with:

| Package | Description |
|---------|-------------|
| **[packages/server](packages/server/README.md)** | Core logic & CLI — workspaces, OpenCode proxy, API, auth, speech |
| **[packages/ui](packages/ui/README.md)** | SolidJS frontend — reactive, fast, beautiful |
| **[packages/electron-app](packages/electron-app/README.md)** | Desktop shell — process management, IPC, native dialogs |
| **[packages/tauri-app](packages/tauri-app)** | Tauri desktop shell (experimental) |

### Quick Start

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## Troubleshooting

<details>
<summary><strong>macOS: "EmbeddedCowork.app is damaged and can't be opened"</strong></summary>

Gatekeeper flag due to missing notarization. Clear the quarantine attribute:

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

On Intel Macs, also check **System Settings → Privacy & Security** on first launch.
</details>

<details>
<summary><strong>Linux (Wayland + NVIDIA): Tauri App closes immediately</strong></summary>

WebKitGTK DMA-BUF/GBM issue. Run with:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

See full workaround in the original README.
</details>

---

## Community

[![Star History](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

感谢以下开源项目：

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad) [opencode](https://github.com/anomalyco/opencode) 


**Built with ♥ by [VividCodeAI](https://github.com/vividcode-ai)** · [MIT License](LICENSE)
