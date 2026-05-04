# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-red)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## OpenCode の AI コーディングコックピット

EmbeddedCowork は OpenCode をターミナルツールから**プレミアムデスクトップワークスペース**へと変革します — AI コーディングセッションに何時間も没頭し、コントロール、スピード、明瞭さを必要とする開発者のために構築されました。

> OpenCode がエンジンを提供し、EmbeddedCowork がコックピットを提供します。

![マルチインスタンスワークスペース](docs/screenshots/newSession.png)

---

## 機能

- **🚀 マルチインスタンスワークスペース**
- **🌐 リモートアクセス**
- **🧠 セッション管理**
- **🎙️ 音声入力と音声合成**
- **🌳 Git Worktrees**
- **💬 リッチなメッセージ体験**
- **🧩 SideCars**
- **⌨️ コマンドパレット**
- **📁 ファイルシステムブラウザ**
- **🔐 認証とセキュリティ**
- **🔔 通知**
- **🎨 テーマ**
- **🌍 国際化**

---

## はじめに

### 🖥️ デスクトップアプリ

Electron と Tauri の両方のビルドを用意しています — お好みに合わせてお選びください。

[Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases) から最新のインストーラーをダウンロードしてください。

| プラットフォーム | 形式 |
|----------------|------|
| macOS | DMG、ZIP（ユニバーサル：Intel + Apple Silicon） |
| Windows | NSIS インストーラー、ZIP（x64、ARM64） |
| Linux | AppImage、deb、tar.gz（x64、ARM64） |

### 💻 EmbeddedCowork サーバー

ローカルサーバーとして実行し、ブラウザ経由でアクセスします。リモート開発に最適です。

```bash
npx @vividcodeai/embeddedcowork --launch
```

フラグ、TLS、認証、リモートアクセスについては[サーバードキュメント](packages/server/README.md)を参照してください。

### 🧪 開発版

`dev` ブランチからの最先端ビルド：

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars を使用すると、ローカルの Web ツールを EmbeddedCowork 内でタブとして開くことができます。

<details>
<summary><strong>設定</strong></summary>

- **名前**: EmbeddedCowork で表示される名前
- **ポート**: `127.0.0.1:<port>` で動作するローカル HTTP または HTTPS サービス
- **ベースパス**: `/sidecars/:id` 下にマウント
- **プレフィックスモード**:
  - **プレフィックスを保持**: 完全な `/sidecars/:id/...` パスを上流に転送
  - **プレフィックスを除去**: リクエストを上流に転送する前に `/sidecars/:id` を削除

</details>

<details>
<summary><strong>VSCode（OpenVSCode Server）</strong></summary>

Docker で実行：

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

SideCar として追加：

- **名前**: `VSCode`
- **ポート**: `http://127.0.0.1:8000`
- **ベースパス**: `/sidecars/vscode`
- **プレフィックスモード**: `プレフィックスを保持`

</details>

<details>
<summary><strong>ターミナル（ttyd）</strong></summary>

実行方法：

```bash
ttyd --writable zsh
```

SideCar として追加：

- **名前**: `Terminal`
- **ポート**: `http://127.0.0.1:7681`
- **ベースパス**: `/sidecars/terminal`
- **プレフィックスモード**: `プレフィックスを除去`

</details>

---

## システム要件

- **[OpenCode CLI](https://opencode.ai)** — インストール済みで `PATH` に含まれている必要があります
- **Node.js 18+** — サーバーモードまたはソースからのビルドに必要

---

## 開発

EmbeddedCowork は以下のモノレポ構成です：

| パッケージ | 説明 |
|-----------|------|
| **[packages/server](packages/server/README.md)** | コアロジックと CLI — ワークスペース、OpenCode プロキシ、API、認証、音声 |
| **[packages/ui](packages/ui/README.md)** | SolidJS フロントエンド — リアクティブ、高速、美しい |
| **[packages/electron-app](packages/electron-app/README.md)** | デスクトップシェル — プロセス管理、IPC、ネイティブダイアログ |
| **[packages/tauri-app](packages/tauri-app)** | Tauri デスクトップシェル（実験的） |

### クイックスタート

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## トラブルシューティング

<details>
<summary><strong>macOS：「EmbeddedCowork.app は壊れているため開けません」</strong></summary>

公証がないための Gatekeeper フラグです。検疫属性をクリアしてください：

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

Intel Mac では、初回起動時に**システム設定 → プライバシーとセキュリティ**も確認してください。
</details>

<details>
<summary><strong>Linux（Wayland + NVIDIA）：Tauri アプリがすぐに閉じる</strong></summary>

WebKitGTK DMA-BUF/GBM の問題です。以下で実行してください：

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

完全な回避策は元の README を参照してください。
</details>

---

## コミュニティ

[![スター履歴](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## 謝辞

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**♥ を込めて [VividCodeAI](https://github.com/vividcode-ai) が制作** · [MIT ライセンス](LICENSE)
