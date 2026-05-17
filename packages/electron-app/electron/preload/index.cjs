const { contextBridge, ipcRenderer, webUtils } = require("electron")

function resolveWindowContext() {
  const prefix = "--embeddedcowork-window-context="
  const arg = process.argv.find((value) => typeof value === "string" && value.startsWith(prefix))
  const context = arg ? arg.slice(prefix.length) : "local"
  return context === "remote" ? "remote" : "local"
}

function resolveRuntimeHost(windowContext) {
  return "electron"
}

const windowContext = resolveWindowContext()

const localElectronAPI = {
  onCliStatus: (callback) => {
    ipcRenderer.on("cli:status", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("cli:status")
  },
  onCliError: (callback) => {
    ipcRenderer.on("cli:error", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("cli:error")
  },
  getCliStatus: () => ipcRenderer.invoke("cli:getStatus"),
  restartCli: () => ipcRenderer.invoke("cli:restart"),
  openDialog: (options) => ipcRenderer.invoke("dialog:open", options),
  getDirectoryPaths: (paths) => ipcRenderer.invoke("filesystem:getDirectoryPaths", paths),
  getPathForFile: (file) => {
    try {
      return webUtils.getPathForFile(file)
    } catch {
      return null
    }
  },
  requestMicrophoneAccess: () => ipcRenderer.invoke("media:requestMicrophoneAccess"),
  setWakeLock: (enabled) => ipcRenderer.invoke("power:setWakeLock", Boolean(enabled)),
  showNotification: (payload) => ipcRenderer.invoke("notifications:show", payload),
  openRemoteWindow: (payload) => ipcRenderer.invoke("remote:openWindow", payload),
  // ── Auto-update API ──
  onUpdateStatus: (callback) => {
    ipcRenderer.on("update:status", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("update:status")
  },
  onUpdateAvailable: (callback) => {
    ipcRenderer.on("update:available", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("update:available")
  },
  onUpdateProgress: (callback) => {
    ipcRenderer.on("update:progress", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("update:progress")
  },
  onUpdateReady: (callback) => {
    ipcRenderer.on("update:ready", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("update:ready")
  },
  onUpdateError: (callback) => {
    ipcRenderer.on("update:error", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("update:error")
  },
  onRollbackNeeded: (callback) => {
    ipcRenderer.on("rollback:needed", (_, data) => callback(data))
    return () => ipcRenderer.removeAllListeners("rollback:needed")
  },
  checkForUpdates: () => ipcRenderer.invoke("update:checkNow"),
  installUpdate: () => ipcRenderer.invoke("update:installNow"),
  rollbackUpdate: () => ipcRenderer.invoke("update:rollback"),
}

const remoteElectronAPI = {
  requestMicrophoneAccess: localElectronAPI.requestMicrophoneAccess,
  setWakeLock: localElectronAPI.setWakeLock,
  showNotification: localElectronAPI.showNotification,
  // Remote windows only need update notification, not install trigger
  onUpdateStatus: localElectronAPI.onUpdateStatus,
  onUpdateAvailable: localElectronAPI.onUpdateAvailable,
  onUpdateProgress: localElectronAPI.onUpdateProgress,
  onUpdateReady: localElectronAPI.onUpdateReady,
  onUpdateError: localElectronAPI.onUpdateError,
  onRollbackNeeded: localElectronAPI.onRollbackNeeded,
}

contextBridge.exposeInMainWorld(
  "electronAPI",
  windowContext === "local" ? localElectronAPI : remoteElectronAPI,
)
contextBridge.exposeInMainWorld("__EMBEDDEDCOWORK_WINDOW_CONTEXT__", windowContext)
contextBridge.exposeInMainWorld("__EMBEDDEDCOWORK_RUNTIME_HOST__", resolveRuntimeHost(windowContext))
