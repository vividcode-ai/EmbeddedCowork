import { app, BrowserWindow, Menu, nativeImage, session, shell, dialog } from "electron"
import pkg from "electron-updater"
const { autoUpdater } = pkg

import { existsSync, mkdirSync } from "fs"
import { dirname, join } from "path"
import { fileURLToPath } from "url"
import { createApplicationMenu } from "./menu"
import { setupCliIPC } from "./ipc"
import { configureMediaPermissionHandlers } from "./permissions"
import { CliProcessManager } from "./process-manager"

const mainFilename = fileURLToPath(import.meta.url)
const mainDirname = dirname(mainFilename)

const isMac = process.platform === "darwin"

function configureDevStoragePaths() {
  if (app.isPackaged) {
    return
  }

  const appName = "EmbeddedCowork"

  try {
    app.setName(appName)

    const userDataPath = join(app.getPath("appData"), appName)
    const sessionDataPath = join(userDataPath, "session-data")

    mkdirSync(userDataPath, { recursive: true })
    mkdirSync(sessionDataPath, { recursive: true })

    app.setPath("userData", userDataPath)
    app.setPath("sessionData", sessionDataPath)

    app.commandLine.appendSwitch("disk-cache-size", String(50 * 1024 * 1024))
  } catch (error) {
    console.warn("[cli] failed to configure dev storage paths", error)
  }
}

configureDevStoragePaths()

const cliManager = new CliProcessManager()
let loadingWindow: BrowserWindow | null = null
let mainWindow: BrowserWindow | null = null
let currentCliUrl: string | null = null
let pendingCliUrl: string | null = null
const remoteWindowOrigins = new Map<number, Set<string>>()
const insecureWindowOrigins = new Map<number, Set<string>>()

if (isMac) {
  app.commandLine.appendSwitch("disable-spell-checking")
}

const gotTheLock = app.requestSingleInstanceLock()
if (!gotTheLock) {
  app.quit()
}

function getIconPath() {
  if (app.isPackaged) {
    return join(process.resourcesPath, "icon.png")
  }

  return join(mainDirname, "../resources/icon.png")
}

type LoadingTarget =
  | { type: "url"; source: string }
  | { type: "file"; source: string }

function resolveDevLoadingUrl(): string | null {
  if (app.isPackaged) {
    return null
  }
  const devBase = process.env.VITE_DEV_SERVER_URL || process.env.ELECTRON_RENDERER_URL
  if (!devBase) {
    return null
  }

  try {
    const normalized = devBase.endsWith("/") ? devBase : `${devBase}/`
    return new URL("loading.html", normalized).toString()
  } catch (error) {
    console.warn("[cli] failed to construct dev loading URL", devBase, error)
    return null
  }
}

function resolveLoadingTarget(): LoadingTarget {
  const devUrl = resolveDevLoadingUrl()
  if (devUrl) {
    return { type: "url", source: devUrl }
  }
  const filePath = resolveLoadingFilePath()
  return { type: "file", source: filePath }
}

function resolveLoadingFilePath() {
  const candidates = [
    join(app.getAppPath(), "dist/renderer/loading.html"),
    join(process.resourcesPath, "dist/renderer/loading.html"),
    join(mainDirname, "../dist/renderer/loading.html"),
  ]

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate
    }
  }

  return join(app.getAppPath(), "dist/renderer/loading.html")
}

function loadLoadingScreen(window: BrowserWindow) {
  const target = resolveLoadingTarget()
  const loader =
    target.type === "url"
      ? window.loadURL(target.source)
      : window.loadFile(target.source)

  loader.catch((error) => {
    console.error("[cli] failed to load loading screen:", error)
  })

  return loader
}

function getAllowedRendererOrigins(window?: BrowserWindow | null): string[] {
  const origins = new Set<string>()
  if (window) {
    for (const origin of remoteWindowOrigins.get(window.id) ?? []) {
      origins.add(origin)
    }
  }
  const rendererCandidates = [currentCliUrl, process.env.VITE_DEV_SERVER_URL, process.env.ELECTRON_RENDERER_URL]
  for (const candidate of rendererCandidates) {
    if (!candidate) {
      continue
    }
    try {
      origins.add(new URL(candidate).origin)
    } catch (error) {
      console.warn("[cli] failed to parse origin for", candidate, error)
    }
  }
  return Array.from(origins)
}

function shouldOpenExternally(url: string, window?: BrowserWindow | null): boolean {
  try {
    const parsed = new URL(url)
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return true
    }
    const allowedOrigins = getAllowedRendererOrigins(window)
    return !allowedOrigins.includes(parsed.origin)
  } catch {
    return false
  }
}

function setupNavigationGuards(window: BrowserWindow) {
  const handleExternal = (url: string) => {
    shell.openExternal(url).catch((error) => console.error("[cli] failed to open external URL", url, error))
  }

  window.webContents.setWindowOpenHandler(({ url }) => {
    if (shouldOpenExternally(url, window)) {
      handleExternal(url)
      return { action: "deny" }
    }
    return { action: "allow" }
  })

  window.webContents.on("will-navigate", (event, url) => {
    if (shouldOpenExternally(url, window)) {
      event.preventDefault()
      handleExternal(url)
    }
  })
}

function setWindowAllowedOrigin(window: BrowserWindow, url: string) {
  try {
    const origin = new URL(url).origin
    remoteWindowOrigins.set(window.id, new Set([origin]))
  } catch (error) {
    console.warn("[cli] failed to store allowed origin", url, error)
  }
}

function clearWindowAllowedOrigin(window: BrowserWindow) {
  remoteWindowOrigins.delete(window.id)
}

function addWindowInsecureOrigin(window: BrowserWindow, url: string) {
  try {
    const origin = new URL(url).origin
    insecureWindowOrigins.set(window.id, new Set([origin]))
  } catch (error) {
    console.warn("[cli] failed to store insecure origin", url, error)
  }
}

function clearWindowInsecureOrigin(window: BrowserWindow) {
  insecureWindowOrigins.delete(window.id)
}

function isInsecureOriginAllowed(url: string) {
  try {
    const targetOrigin = new URL(url).origin
    for (const origins of insecureWindowOrigins.values()) {
      if (origins.has(targetOrigin)) {
        return true
      }
    }
  } catch {
    return false
  }

  return false
}

let cachedPreloadPath: string | null = null
function getPreloadPath() {
  if (cachedPreloadPath && existsSync(cachedPreloadPath)) {
    return cachedPreloadPath
  }

  const candidates = [
    join(process.resourcesPath, "preload/index.js"),
    join(mainDirname, "../preload/index.js"),
    join(mainDirname, "../preload/index.cjs"),
    join(mainDirname, "../../preload/index.cjs"),
    join(mainDirname, "../../electron/preload/index.cjs"),
    join(app.getAppPath(), "preload/index.cjs"),
    join(app.getAppPath(), "electron/preload/index.cjs"),
  ]

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      cachedPreloadPath = candidate
      return candidate
    }
  }

  return join(mainDirname, "../preload/index.js")
}

function createLoadingWindow() {
  const iconPath = getIconPath()

  loadingWindow = new BrowserWindow({
    width: 800,
    height: 600,
    frame: false,
    transparent: true,
    resizable: false,
    center: true,
    icon: iconPath,
    show: false,
    alwaysOnTop: false,
    webPreferences: {
      preload: getPreloadPath(),
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: !isMac,
    },
  })

  loadLoadingScreen(loadingWindow)
  loadingWindow.once("ready-to-show", () => {
    loadingWindow?.show()
  })

  loadingWindow.on("closed", () => {
    loadingWindow = null
  })
}

function createMainWindow() {
  const backgroundColor = "#1a1a1a"
  const iconPath = getIconPath()

  mainWindow = new BrowserWindow({
    width: 800,
    height: 600,
    minWidth: 800,
    minHeight: 600,
    backgroundColor,
    icon: iconPath,
    show: false,
    webPreferences: {
      preload: getPreloadPath(),
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: !isMac,
      additionalArguments: ["--embeddedcowork-window-context=local"],
    },
  })

  const window = mainWindow

  setupNavigationGuards(window)

  if (isMac) {
    window.webContents.session.setSpellCheckerEnabled(false)
  }

  currentCliUrl = null
  clearWindowAllowedOrigin(window)

  if (process.env.NODE_ENV === "development") {
    window.webContents.openDevTools({ mode: "detach" })
  }

  Menu.setApplicationMenu(null)
  setupCliIPC(window, cliManager)

  window.on("closed", () => {
    clearWindowAllowedOrigin(window)
    clearWindowInsecureOrigin(window)
    mainWindow = null
    currentCliUrl = null
    pendingCliUrl = null
  })
}

function showLoadingScreen(force = false) {
  if (!loadingWindow || loadingWindow.isDestroyed()) {
    return
  }

  loadLoadingScreen(loadingWindow)
}

function isBootstrapTokenUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return parsed.pathname === "/auth/token" && parsed.hash.length > 1
  } catch {
    return false
  }
}

function startCliPreload(url: string) {
  if (!mainWindow || mainWindow.isDestroyed()) {
    pendingCliUrl = url
    return
  }

  if (currentCliUrl === url) {
    return
  }

  pendingCliUrl = url
  showLoadingScreen(true)

  finalizeCliSwap(url)
}

function finalizeCliSwap(url: string) {
  if (!mainWindow || mainWindow.isDestroyed()) {
    pendingCliUrl = url
    return
  }

  if (currentCliUrl === url) {
    return
  }

  currentCliUrl = url
  setWindowAllowedOrigin(mainWindow, url)
  pendingCliUrl = null

  mainWindow.loadURL(url).then(() => {
    if (loadingWindow && !loadingWindow.isDestroyed()) {
      loadingWindow.destroy()
      loadingWindow = null
    }
    mainWindow?.maximize()
    createApplicationMenu(mainWindow!)
  }).catch((error) => console.error("[cli] failed to load CLI view:", error))
}

function buildRemoteWindowTitle(name: string, baseUrl: string) {
  try {
    const parsed = new URL(baseUrl)
    return `${name} - ${parsed.host}`
  } catch {
    return `${name} - ${baseUrl}`
  }
}

function buildRemoteErrorHtml(name: string, baseUrl: string, message: string) {
  const escapedName = name.replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[char] ?? char))
  const escapedUrl = baseUrl.replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[char] ?? char))
  const escapedMessage = message.replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[char] ?? char))
  return `<!doctype html><html><head><meta charset="utf-8" /><title>${escapedName}</title><style>body{margin:0;background:#111827;color:#f9fafb;font-family:Inter,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh;padding:24px}main{max-width:560px;width:100%;background:rgba(17,24,39,.88);border:1px solid rgba(255,255,255,.08);border-radius:20px;padding:28px;box-shadow:0 25px 60px rgba(0,0,0,.45)}h1{margin:0 0 10px;font-size:1.5rem}p{margin:0 0 10px;color:#cbd5e1;line-height:1.5}code{display:block;margin-top:16px;padding:12px 14px;border-radius:12px;background:#0f172a;color:#bfdbfe;overflow:auto}</style></head><body><main><h1>${escapedName}</h1><p>Could not connect to the remote server.</p><p>${escapedMessage}</p><code>${escapedUrl}</code></main></body></html>`
}

async function openRemoteWindow(payload: { id: string; name: string; baseUrl: string; skipTlsVerify: boolean }) {
  const targetUrl = new URL(payload.baseUrl)
  const title = buildRemoteWindowTitle(payload.name, payload.baseUrl)
  const window = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    backgroundColor: "#1a1a1a",
    icon: getIconPath(),
    title,
    webPreferences: {
      preload: getPreloadPath(),
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: !isMac,
      additionalArguments: ["--embeddedcowork-window-context=remote"],
    },
  })

  setWindowAllowedOrigin(window, targetUrl.toString())
  if (payload.skipTlsVerify) {
    addWindowInsecureOrigin(window, targetUrl.toString())
  }

  setupNavigationGuards(window)
  window.on("closed", () => {
    clearWindowAllowedOrigin(window)
    clearWindowInsecureOrigin(window)
  })

  try {
    await window.loadURL(targetUrl.toString())
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(buildRemoteErrorHtml(payload.name, payload.baseUrl, message))}`)
  }
}

async function startCli() {
  try {
    const devMode = !app.isPackaged
    console.info("[cli] start requested (dev mode:", devMode, ")")
    await cliManager.start({ dev: devMode })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error("[cli] start failed:", message)
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("cli:error", { message })
    }
  }
}

cliManager.on("ready", (status) => {
  if (!status.url) {
    return
  }

  startCliPreload(status.url)
  autoUpdater.checkForUpdates()
})

cliManager.on("status", (status) => {
  if (status.state !== "ready") {
    showLoadingScreen()
  }
})

if (isMac) {
  app.on("web-contents-created", (_, contents) => {
    contents.session.setSpellCheckerEnabled(false)
  })
}

app.whenReady().then(() => {
  try {
    app.setAppUserModelId("ai.vividcode.embeddedcowork.client")
  } catch {
    // ignore
  }

  autoUpdater.autoDownload = false
  autoUpdater.autoInstallOnAppQuit = true

  createLoadingWindow()
  createMainWindow()
  ;(mainWindow as BrowserWindow & { __embeddedcoworkOpenRemoteWindow?: typeof openRemoteWindow }).__embeddedcoworkOpenRemoteWindow = openRemoteWindow

  if (isMac) {
    session.defaultSession.setSpellCheckerEnabled(false)
    configureMediaPermissionHandlers(getAllowedRendererOrigins)
    app.on("browser-window-created", (_, window) => {
      window.webContents.session.setSpellCheckerEnabled(false)
    })

    if (app.dock) {
      const dockIcon = nativeImage.createFromPath(getIconPath())
      if (!dockIcon.isEmpty()) {
        app.dock.setIcon(dockIcon)
      }
    }
  }

  setTimeout(() => {
    void startCli()
  }, 0)

  app.on("certificate-error", (event, _webContents, url, error, _certificate, callback) => {
    if (isInsecureOriginAllowed(url)) {
      event.preventDefault()
      console.warn("[cli] allowing insecure remote certificate for", url, error)
      callback(true)
      return
    }
    callback(false)
  })

  app.on("activate", () => {
    if (!mainWindow || mainWindow.isDestroyed()) {
      createMainWindow()
    }
    if (!loadingWindow || loadingWindow.isDestroyed()) {
      createLoadingWindow()
    }
  })

  app.on("second-instance", () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore()
      mainWindow.focus()
    }
  })
})

autoUpdater.on("update-available", (info) => {
  const btnIdx = dialog.showMessageBoxSync(mainWindow!, {
    type: "info",
    title: "更新可用",
    message: `EmbeddedCowork v${info.version} 可用`,
    detail: "是否下载并安装？",
    buttons: ["下载", "稍后"],
    defaultId: 0,
    cancelId: 1,
  })
  if (btnIdx === 0) autoUpdater.downloadUpdate()
})

autoUpdater.on("update-not-available", () => {
})

autoUpdater.on("download-progress", (progress) => {
  const pct = Math.round(progress.percent)
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.setProgressBar(progress.percent / 100)
    mainWindow.webContents.send("update:progress", pct)
  }
})

autoUpdater.on("update-downloaded", () => {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.setProgressBar(-1)
  }

  const btnIdx = dialog.showMessageBoxSync(mainWindow!, {
    type: "info",
    title: "更新已下载",
    message: "新版本已就绪",
    detail: "重启应用以完成安装。",
    buttons: ["立即重启", "稍后"],
    defaultId: 0,
    cancelId: 1,
  })
  if (btnIdx === 0) setImmediate(() => autoUpdater.quitAndInstall())
})

autoUpdater.on("error", (error) => {
  console.error("[autoUpdater]", error.message)
})

app.on("before-quit", async () => {
  await cliManager.stop().catch(() => {})
})

app.on("window-all-closed", () => {
  // EmbeddedCowork supports a single window; closing it should quit the app on all platforms.
  app.quit()
})
