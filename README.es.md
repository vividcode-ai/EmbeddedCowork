# EmbeddedCowork

[![EN](https://img.shields.io/badge/EN-English-blue)](README.en.md) [![ES](https://img.shields.io/badge/ES-Espa%C3%B1ol-red)](README.es.md) [![FR](https://img.shields.io/badge/FR-Fran%C3%A7ais-blue)](README.fr.md) [![RU](https://img.shields.io/badge/RU-%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-blue)](README.ru.md) [![JA](https://img.shields.io/badge/JA-%E6%97%A5%E6%9C%AC%E8%AA%9E-blue)](README.ja.md) [![ZH](https://img.shields.io/badge/ZH-%E4%B8%AD%E6%96%87-blue)](README.md) [![HE](https://img.shields.io/badge/HE-%D7%A2%D7%91%D7%A8%D7%99%D7%AA-blue)](README.he.md)

## La Cabina de Vuelo de IA para OpenCode

EmbeddedCowork transforma OpenCode de una herramienta de terminal en un **espacio de trabajo premium de escritorio** — construido para desarrolladores que viven dentro de sesiones de codificación con IA durante horas y necesitan control, velocidad y claridad.

> OpenCode te da el motor. EmbeddedCowork te da la cabina de vuelo.

![Espacio de trabajo multiinstancia](docs/screenshots/newSession.png)

---

## Características

- **🚀 Espacio de Trabajo Multiinstancia**
- **🌐 Acceso Remoto**
- **🧠 Gestión de Sesiones**
- **🎙️ Entrada de Voz y Habla**
- **🌳 Git Worktrees**
- **💬 Experiencia de Mensajes Enriquecida**
- **🧩 SideCars**
- **⌨️ Paleta de Comandos**
- **📁 Explorador del Sistema de Archivos**
- **🔐 Autenticación y Seguridad**
- **🔔 Notificaciones**
- **🎨 Temas**
- **🌍 Internacionalización**

---

## Primeros Pasos

### 🖥️ Aplicación de Escritorio

Disponible como builds de Electron y Tauri — elige según tu preferencia.

Descarga el instalador más reciente para tu plataforma desde [Releases](https://github.com/vividcode-ai/EmbeddedCowork/releases).

| Plataforma | Formatos |
|------------|----------|
| macOS | DMG, ZIP (Universal: Intel + Apple Silicon) |
| Windows | Instalador NSIS, ZIP (x64, ARM64) |
| Linux | AppImage, deb, tar.gz (x64, ARM64) |

### 💻 Servidor EmbeddedCowork

Ejecuta como servidor local y accede vía navegador. Perfecto para desarrollo remoto.

```bash
npx @vividcodeai/embeddedcowork --launch
```

Consulta la [Documentación del Servidor](packages/server/README.md) para flags, TLS, autenticación y acceso remoto.

### 🧪 Versiones de Desarrollo

Builds de vanguardia desde la rama `dev`:

```bash
npx @vividcodeai/embeddedcowork-dev --launch
```

---

## SideCars

SideCars te permite abrir herramientas web locales dentro de EmbeddedCowork como pestañas.

<details>
<summary><strong>Configuración</strong></summary>

- **Nombre**: Nombre mostrado en EmbeddedCowork
- **Puerto**: Servicio HTTP o HTTPS local ejecutándose en `127.0.0.1:<port>`
- **Ruta base**: Montado bajo `/sidecars/:id`
- **Modo de prefijo**:
  - **Conservar prefijo** envía la ruta completa `/sidecars/:id/...` al upstream
  - **Eliminar prefijo** quita `/sidecars/:id` antes de reenviar la solicitud al upstream

</details>

<details>
<summary><strong>VSCode (OpenVSCode Server)</strong></summary>

Ejecutar con Docker:

```bash
docker run -it --init -p 8000:3000 -v "${HOME}:${HOME}:cached" -e HOME=${HOME} gitpod/openvscode-server --server-base-path /sidecars/vscode
```

Agregar SideCar como:

- **Nombre**: `VSCode`
- **Puerto**: `http://127.0.0.1:8000`
- **Ruta base**: `/sidecars/vscode`
- **Modo de prefijo**: `Conservar prefijo`

</details>

<details>
<summary><strong>Terminal (ttyd)</strong></summary>

Ejecutar con:

```bash
ttyd --writable zsh
```

Agregar SideCar como:

- **Nombre**: `Terminal`
- **Puerto**: `http://127.0.0.1:7681`
- **Ruta base**: `/sidecars/terminal`
- **Modo de prefijo**: `Eliminar prefijo`

</details>

---

## Requisitos

- **[OpenCode CLI](https://opencode.ai)** — debe estar instalado y en tu `PATH`
- **Node.js 18+** — para modo servidor o compilación desde el código fuente

---

## Desarrollo

EmbeddedCowork es un monorepo construido con:

| Paquete | Descripción |
|---------|-------------|
| **[packages/server](packages/server/README.md)** | Lógica principal y CLI — espacios de trabajo, proxy OpenCode, API, autenticación, voz |
| **[packages/ui](packages/ui/README.md)** | Frontend SolidJS — reactivo, rápido, hermoso |
| **[packages/electron-app](packages/electron-app/README.md)** | Shell de escritorio — gestión de procesos, IPC, diálogos nativos |
| **[packages/tauri-app](packages/tauri-app)** | Shell de escritorio Tauri (experimental) |

### Inicio Rápido

```bash
git clone https://github.com/vividcode-ai/EmbeddedCowork.git
cd EmbeddedCowork
npm install
npm run dev
```

---

## Solución de Problemas

<details>
<summary><strong>macOS: "EmbeddedCowork.app está dañado y no se puede abrir"</strong></summary>

Marca de Gatekeeper debido a falta de notarización. Limpia el atributo de cuarentena:

```bash
xattr -dr com.apple.quarantine /Applications/EmbeddedCowork.app
```

En Macs Intel, verifica también **Configuración del Sistema → Privacidad y Seguridad** al primer inicio.
</details>

<details>
<summary><strong>Linux (Wayland + NVIDIA): La app Tauri se cierra inmediatamente</strong></summary>

Problema de WebKitGTK DMA-BUF/GBM. Ejecutar con:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 embeddedcowork
```

Consulta la solución completa en el README original.
</details>

---

## Comunidad

[![Historial de Estrellas](https://api.star-history.com/svg?repos=vividcode-ai/EmbeddedCowork&type=Date)](https://star-history.com/#vividcode-ai/EmbeddedCowork&Date)

---

## Agradecimientos

- [CodeNomad](https://github.com/NeuralNomadsAI/CodeNomad)
- [opencode](https://github.com/anomalyco/opencode)

**Construido con ♥ por [VividCodeAI](https://github.com/vividcode-ai)** · [Licencia MIT](LICENSE)
