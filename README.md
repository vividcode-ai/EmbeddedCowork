# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-red)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## OpenCode 的 AI 编程驾驶舱

EmbeddedCowork 将 OpenCode 从终端工具转变为**高端桌面工作空间**——专为那些在 AI 编程会话中沉浸数小时、需要掌控力、速度和清晰度的开发者而打造。

> OpenCode 提供引擎，EmbeddedCowork 提供驾驶舱。

![多实例工作空间](docs/screenshots/newSession.png)

---

## 功能特性

- **🚀 多实例工作空间**
- **🌐 远程访问**
- **🧠 会话管理**
- **🎙️ 语音输入与语音功能**
- **🌳 Git Worktrees**
- **💬 富消息体验**
- **🧩 侧边应用 (SideCars)**
- **⌨️ 命令面板**
- **📁 文件系统浏览器**
- **🔐 认证与安全**
- **🔔 通知**
- **🎨 主题定制**
- **🌍 国际化**

---

## 快速开始

### 🖥️ 桌面应用

提供 Electron 和 Tauri 两种构建版本——可根据偏好选择。

从 [Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases) 下载适用于您平台的最新安装包。

| 平台 | 格式 |
|------|------|
| macOS | DMG、ZIP（通用：Intel + Apple Silicon） |
| Windows | NSIS 安装程序、ZIP（x64、ARM64） |
| Linux | AppImage、deb、tar.gz（x64、ARM64） |

### 💻 EmbeddedCowork 服务器

作为本地服务器运行，通过浏览器访问。适用于远程开发。

```bash
npx @vividcodeai/embeddedcowork --launch
```

参见[服务器文档](packages/server/README.md)了解命令行参数、TLS、认证和远程访问配置。

### 🧪 开发版

来自 `dev` 分支的最新构建版本：

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## 侧边应用 (SideCars)

SideCars 允许您将本地 Web 工具以标签页形式在 EmbeddedCowork 中打开。

<details>
<summary><strong>配置说明</strong></summary>

- **名称**：EmbeddedCowork 中显示的标签名称
- **端口**：运行在 `127.0.0.1:<port>` 的本地 HTTP 或 HTTPS 服务
- **基础路径**：挂载在 `/sidecars/:id` 下
- **前缀模式**：
  - **保留前缀**：将完整的 `/sidecars/:id/...` 路径转发到上游服务
  - **去除前缀**：在转发请求到上游服务之前移除 `/sidecars/:id` 前缀

</details>

<details>
<summary><strong>VSCode（OpenVSCode Server）</strong></summary>

使用 Docker 运行：

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

添加 SideCar 配置：

- **名称**：`VSCode`
- **端口**：`http://127.0.0.1:8000`
- **基础路径**：`/sidecars/vscode`
- **前缀模式**：`保留前缀`

</details>

<details>
<summary><strong>终端（ttyd）</strong></summary>

运行方式：

```bash
ttyd --writable zsh
```

添加 SideCar 配置：

- **名称**：`Terminal`
- **端口**：`http://127.0.0.1:7681`
- **基础路径**：`/sidecars/terminal`
- **前缀模式**：`去除前缀`

</details>

---

## 系统要求

- **[OpenCode CLI](https://opencode.ai)** — 必须已安装且在 `PATH` 环境变量中
- **Node.js 18+** — 用于服务器模式或从源码构建

---

## 开发指南

EmbeddedCowork 是一个基于以下技术构建的 monorepo（多包仓库）：

| 包 | 说明 |
|----|------|
| **[packages/server](packages/server/README.md)** | 核心逻辑与 CLI——工作空间、OpenCode 代理、API、认证、语音 |
| **[packages/ui](packages/ui/README.md)** | SolidJS 前端——响应式、快速、美观 |
| **[packages/electron-app](packages/electron-app/README.md)** | 桌面壳——进程管理、IPC、原生对话框 |
| **[packages/tauri-app](packages/tauri-app)** | Tauri 桌面壳（实验性） |

### 快速启动

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## 故障排除

<details>
<summary><strong>macOS："EmbeddedCowork.app 已损坏，无法打开"</strong></summary>

由于缺少公证导致 Gatekeeper 标记。清除隔离属性：

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

在 Intel Mac 上，首次启动时还需检查**系统设置 → 隐私与安全性**。
</details>

<details>
<summary><strong>Linux（Wayland + NVIDIA）：Tauri 应用立即关闭</strong></summary>

WebKitGTK DMA-BUF/GBM 问题。使用以下方式运行：

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

参见原始 README 中的完整解决方案。
</details>

---

## 社区

[![Star 历史](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## 致谢

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**由 [VividCodeAI](https://github.com/vividcode-ai) 用心构建** · [MIT 许可证](LICENSE)
