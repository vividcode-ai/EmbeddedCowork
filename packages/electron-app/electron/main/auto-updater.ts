import pkg from "electron-updater"
import { BrowserWindow, app } from "electron"
import { ElectronRollbackManager } from "./rollback"

const { autoUpdater } = pkg

export class AppAutoUpdater {
  private rollbackManager: ElectronRollbackManager
  private mainWindow: BrowserWindow | null = null
  private cleanupFunctions: Array<() => void> = []

  constructor() {
    this.rollbackManager = new ElectronRollbackManager()

    autoUpdater.autoDownload = true
    autoUpdater.allowPrerelease = false
    autoUpdater.allowDowngrade = false
    autoUpdater.autoInstallOnAppQuit = false

    this.setupListeners()
  }

  setMainWindow(window: BrowserWindow) {
    this.mainWindow = window
  }

  private send(channel: string, data?: unknown) {
    if (this.mainWindow && !this.mainWindow.isDestroyed()) {
      this.mainWindow.webContents.send(channel, data)
    }
  }

  private setupListeners() {
    autoUpdater.on("checking-for-update", () => {
      this.send("update:status", "checking")
    })

    autoUpdater.on("update-available", (info) => {
      // Backup current version before starting download
      this.rollbackManager.backupCurrentApp(app.getVersion()).catch(() => {
        // Backup best-effort; don't block update if backup fails
        console.warn("[auto-updater] failed to backup current app")
      })
      this.send("update:available", { version: info.version, releaseDate: info.releaseDate })
    })

    autoUpdater.on("update-not-available", () => {
      this.send("update:status", "no-update")
    })

    autoUpdater.on("download-progress", (progress) => {
      this.send("update:progress", {
        percent: progress.percent,
        bytesPerSecond: progress.bytesPerSecond,
        total: progress.total,
        transferred: progress.transferred,
      })
    })

    autoUpdater.on("update-downloaded", (info) => {
      this.rollbackManager.markInstalling(info.version)
      this.send("update:ready", {
        version: info.version,
        releaseNotes: info.releaseNotes,
        releaseDate: info.releaseDate,
      })
    })

    autoUpdater.on("error", (err) => {
      this.send("update:error", { message: err.message })
    })
  }

  init() {
    if (!app.isPackaged) {
      console.log("[auto-updater] skipped (dev mode)")
      return
    }

    // Check for rollback on this startup
    this.checkRollbackOnStartup()

    // Start checking for updates
    autoUpdater.checkForUpdates().catch((err) => {
      console.warn("[auto-updater] initial check failed:", err.message)
    })
  }

  private async checkRollbackOnStartup() {
    try {
      const status = await this.rollbackManager.checkStartup()
      if (status?.needsRollback) {
        this.send("rollback:needed", {
          oldVersion: status.oldVersion,
          newVersion: status.newVersion,
        })
      } else if (status?.cleanStart) {
        // Let the app run for a bit, then confirm if no crash
        setTimeout(() => {
          this.rollbackManager.confirmHealthyStart().catch(() => {})
        }, 30000) // 30-second health window
      }
    } catch {
      // Ignore errors in rollback check
    }
  }

  /** Check for updates (called from menu) */
  checkForUpdates() {
    if (!app.isPackaged) return
    autoUpdater.checkForUpdates().catch((err) => {
      this.send("update:error", { message: err.message })
    })
  }

  /** Install the downloaded update immediately */
  installNow() {
    autoUpdater.quitAndInstall()
  }

  /** Rollback to previous version */
  async rollback() {
    try {
      await this.rollbackManager.rollbackToPrevious()
      app.relaunch()
      app.quit()
    } catch (err) {
      this.send("update:error", { message: err instanceof Error ? err.message : "Rollback failed" })
    }
  }

  /** Mark clean exit on app quit */
  async markCleanExit() {
    try {
      await this.rollbackManager.markCleanExit()
    } catch {
      // best-effort
    }
  }

  onCleanup() {
    for (const fn of this.cleanupFunctions) {
      try { fn() } catch { /* ignore */ }
    }
    this.cleanupFunctions = []
  }
}
