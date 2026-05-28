import pkg from "electron-updater"
import { BrowserWindow, app } from "electron"
import { ElectronRollbackManager } from "./rollback"

const { autoUpdater } = pkg

export class AppAutoUpdater {
  /** Whether an update install is in progress (used by before-quit to let quitAndInstall manage the flow) */
  isUpdating = false

  private rollbackManager: ElectronRollbackManager
  private mainWindow: BrowserWindow | null = null
  private cleanupFunctions: Array<() => void> = []
  private downloadedInstallerPath: string | null = null

  constructor() {
    this.rollbackManager = new ElectronRollbackManager()

    autoUpdater.autoDownload = false
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

  private setProgressBar(value: number) {
    if (this.mainWindow && !this.mainWindow.isDestroyed()) {
      this.mainWindow.setProgressBar(value)
    }
  }

  private setupListeners() {
    autoUpdater.on("checking-for-update", () => {
      this.setProgressBar(-1) // indeterminate (taskbar pulse)
      this.send("update:status", "checking")
    })

    autoUpdater.on("update-available", () => {
      // Backup current version before starting download
      this.rollbackManager.backupCurrentApp(app.getVersion()).catch(() => {
        console.warn("[auto-updater] failed to backup current app")
      })
      this.setProgressBar(0) // 0% – download starting
    })

    autoUpdater.on("update-not-available", () => {
      this.setProgressBar(-1) // clear
      this.send("update:status", "no-update")
    })

    autoUpdater.on("download-progress", (progress) => {
      this.setProgressBar(progress.percent / 100) // 0.0 – 1.0
      this.send("update:progress", {
        percent: progress.percent,
        bytesPerSecond: progress.bytesPerSecond,
        total: progress.total,
        transferred: progress.transferred,
      })
    })

    autoUpdater.on("update-downloaded", (info) => {
      this.setProgressBar(1) // 100% green
      // Clear progress bar after 2 seconds
      setTimeout(() => this.setProgressBar(-1), 2000)
      this.downloadedInstallerPath = info.downloadedFile
      this.rollbackManager.markInstalling(info.version)
      this.send("update:ready", {
        version: info.version,
        releaseNotes: info.releaseNotes,
        releaseDate: info.releaseDate,
      })
    })

    autoUpdater.on("error", (err) => {
      this.setProgressBar(2) // error (red on Windows/macOS)
      setTimeout(() => this.setProgressBar(-1), 3000)
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

  /** Check for updates and download in background. Returns update info. */
  async checkUpdate(): Promise<{ updateAvailable: boolean; version?: string }> {
    if (!app.isPackaged) return { updateAvailable: false }
    try {
      const result = await autoUpdater.checkForUpdates()
      const version = result?.updateInfo?.version
      if (!result?.isUpdateAvailable || !version) {
        return { updateAvailable: false }
      }
      await autoUpdater.downloadUpdate()
      return { updateAvailable: true, version }
    } catch {
      return { updateAvailable: false }
    }
  }

  /** Check for updates with user-visible dialog (called from menu) */
  checkForUpdates() {
    if (!app.isPackaged) return
    autoUpdater.checkForUpdates().catch((err) => {
      this.send("update:error", { message: err.message })
    })
  }

  /** Get the downloaded installer path */
  getInstallerPath(): string | null {
    return this.downloadedInstallerPath
  }

  /** Install the downloaded update immediately */
  installNow() {
    this.isUpdating = true
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
