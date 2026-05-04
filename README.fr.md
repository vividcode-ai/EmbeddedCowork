# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-blue)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-red)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## Le Cockpit IA pour OpenCode

EmbeddedCowork transforme OpenCode d'un outil terminal en un **espace de travail desktop premium** — conçu pour les développeurs qui passent des heures dans des sessions de codage IA et ont besoin de contrôle, rapidité et clarté.

> OpenCode vous donne le moteur. EmbeddedCowork vous donne le cockpit.

![Espace de travail multi-instance](docs/screenshots/newSession.png)

---

## Fonctionnalités

- **🚀 Espace de Travail Multi-Instance**
- **🌐 Accès à Distance**
- **🧠 Gestion des Sessions**
- **🎙️ Saisie Vocale et Synthèse**
- **🌳 Git Worktrees**
- **💬 Expérience de Messagerie Riche**
- **🧩 SideCars**
- **⌨️ Palette de Commandes**
- **📁 Explorateur de Système de Fichiers**
- **🔐 Authentification et Sécurité**
- **🔔 Notifications**
- **🎨 Thèmes**
- **🌍 Internationalisation**

---

## Pour Commencer

### 🖥️ Application de Bureau

Disponible en versions Electron et Tauri — choisissez selon votre préférence.

Téléchargez le dernier installateur pour votre plateforme depuis [Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases).

| Plateforme | Formats |
|------------|---------|
| macOS | DMG, ZIP (Universel : Intel + Apple Silicon) |
| Windows | Installateur NSIS, ZIP (x64, ARM64) |
| Linux | AppImage, deb, tar.gz (x64, ARM64) |

### 💻 Serveur EmbeddedCowork

Exécutez en tant que serveur local et accédez via navigateur. Parfait pour le développement à distance.

```bash
npx @vividcodeai/embeddedcowork --launch
```

Voir la [Documentation du Serveur](packages/server/README.md) pour les flags, TLS, authentification et accès à distance.

### 🧪 Versions de Développement

Builds de pointe depuis la branche `dev` :

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars vous permet d'ouvrir des outils web locaux dans EmbeddedCowork sous forme d'onglets.

<details>
<summary><strong>Configuration</strong></summary>

- **Nom** : Nom d'affichage utilisé dans EmbeddedCowork
- **Port** : Service HTTP ou HTTPS local exécuté sur `127.0.0.1:<port>`
- **Chemin de base** : Monté sous `/sidecars/:id`
- **Mode de préfixe** :
  - **Conserver le préfixe** transmet le chemin complet `/sidecars/:id/...` en amont
  - **Supprimer le préfixe** retire `/sidecars/:id` avant de transmettre la requête en amont

</details>

<details>
<summary><strong>VSCode (OpenVSCode Server)</strong></summary>

Exécuter avec Docker :

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

Ajouter SideCar comme :

- **Nom** : `VSCode`
- **Port** : `http://127.0.0.1:8000`
- **Chemin de base** : `/sidecars/vscode`
- **Mode de préfixe** : `Conserver le préfixe`

</details>

<details>
<summary><strong>Terminal (ttyd)</strong></summary>

Exécuter avec :

```bash
ttyd --writable zsh
```

Ajouter SideCar comme :

- **Nom** : `Terminal`
- **Port** : `http://127.0.0.1:7681`
- **Chemin de base** : `/sidecars/terminal`
- **Mode de préfixe** : `Supprimer le préfixe`

</details>

---

## Prérequis

- **[OpenCode CLI](https://opencode.ai)** — doit être installé et dans votre `PATH`
- **Node.js 18+** — pour le mode serveur ou la compilation depuis les sources

---

## Développement

EmbeddedCowork est un monorepo construit avec :

| Paquet | Description |
|--------|-------------|
| **[packages/server](packages/server/README.md)** | Logique centrale et CLI — espaces de travail, proxy OpenCode, API, authentification, synthèse vocale |
| **[packages/ui](packages/ui/README.md)** | Frontend SolidJS — réactif, rapide, magnifique |
| **[packages/electron-app](packages/electron-app/README.md)** | Shell de bureau — gestion de processus, IPC, dialogues natifs |
| **[packages/tauri-app](packages/tauri-app)** | Shell de bureau Tauri (expérimental) |

### Démarrage Rapide

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## Dépannage

<details>
<summary><strong>macOS : "EmbeddedCowork.app est endommagé et ne peut pas être ouvert"</strong></summary>

Marquage Gatekeeper dû à l'absence de notarisation. Supprimez l'attribut de quarantaine :

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

Sur les Mac Intel, vérifiez également **Réglages Système → Confidentialité et Sécurité** au premier lancement.
</details>

<details>
<summary><strong>Linux (Wayland + NVIDIA) : L'application Tauri se ferme immédiatement</strong></summary>

Problème WebKitGTK DMA-BUF/GBM. Exécutez avec :

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

Voir la solution complète dans le README original.
</details>

---

## Communauté

[![Historique des Étoiles](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## Remerciements

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**Construit avec ♥ par [VividCodeAI](https://github.com/vividcode-ai)** · [Licence MIT](LICENSE)
