import { BrowserWindow, Notification, app, dialog, ipcMain, powerSaveBlocker, type OpenDialogOptions } from "electron"
import fs from "fs"
import { spawn, spawnSync } from "child_process"
import { requestMicrophoneAccess } from "./permissions"
import type { CliProcessManager, CliStatus } from "./process-manager"
import type { AppAutoUpdater } from "./auto-updater"

let wakeLockId: number | null = null

interface DialogOpenRequest {
  mode: "directory" | "file"
  title?: string
  defaultPath?: string
  filters?: Array<{ name?: string; extensions: string[] }>
}

interface DialogOpenResult {
  canceled: boolean
  paths: string[]
}

export function setupCliIPC(mainWindow: BrowserWindow, cliManager: CliProcessManager, appAutoUpdater?: AppAutoUpdater) {
  cliManager.on("status", (status: CliStatus) => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send("cli:status", status)
    }
  })

  cliManager.on("ready", (status: CliStatus) => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send("cli:ready", status)
    }
  })

  cliManager.on("error", (error: Error) => {
    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send("cli:error", { message: error.message })
    }
  })

  ipcMain.handle("cli:getStatus", async () => cliManager.getStatus())

  ipcMain.handle("cli:restart", async () => {
    const devMode = process.env.NODE_ENV === "development"
    await cliManager.stop()
    return cliManager.start({ dev: devMode })
  })

  ipcMain.handle("dialog:open", async (_, request: DialogOpenRequest): Promise<DialogOpenResult> => {
    const properties: OpenDialogOptions["properties"] =
      request.mode === "directory" ? ["openDirectory", "createDirectory"] : ["openFile"]

    const filters = request.filters?.map((filter) => ({
      name: filter.name ?? "Files",
      extensions: filter.extensions,
    }))

    const windowTarget = mainWindow.isDestroyed() ? undefined : mainWindow
    const dialogOptions: OpenDialogOptions = {
      title: request.title,
      defaultPath: request.defaultPath,
      properties,
      filters,
    }
    const result = windowTarget
      ? await dialog.showOpenDialog(windowTarget, dialogOptions)
      : await dialog.showOpenDialog(dialogOptions)

    return { canceled: result.canceled, paths: result.filePaths }
  })

  ipcMain.handle("filesystem:getDirectoryPaths", async (_event, paths: unknown): Promise<string[]> => {
    if (!Array.isArray(paths)) {
      return []
    }

    const directories = paths.filter((value): value is string => {
      if (typeof value !== "string" || value.trim().length === 0) {
        return false
      }
      try {
        return fs.statSync(value).isDirectory()
      } catch {
        return false
      }
    })
    return directories
  })

  ipcMain.handle("power:setWakeLock", async (_event, enabled: boolean): Promise<{ enabled: boolean }> => {
    const next = Boolean(enabled)
    if (next) {
      if (wakeLockId !== null && powerSaveBlocker.isStarted(wakeLockId)) {
        return { enabled: true }
      }
      try {
        wakeLockId = powerSaveBlocker.start("prevent-app-suspension")
      } catch {
        wakeLockId = null
        return { enabled: false }
      }
      return { enabled: true }
    }

    if (wakeLockId !== null) {
      try {
        if (powerSaveBlocker.isStarted(wakeLockId)) {
          powerSaveBlocker.stop(wakeLockId)
        }
      } finally {
        wakeLockId = null
      }
    }
    return { enabled: false }
  })

  ipcMain.handle(
    "media:requestMicrophoneAccess",
    async (): Promise<{ granted: boolean }> => ({ granted: await requestMicrophoneAccess() }),
  )

  ipcMain.handle(
    "remote:openWindow",
    async (
      _event,
      payload: { id: string; name: string; baseUrl: string; skipTlsVerify: boolean },
    ): Promise<{ ok: boolean }> => {
      const opener = (mainWindow as BrowserWindow & {
        __embeddedcoworkOpenRemoteWindow?: (payload: {
          id: string
          name: string
          baseUrl: string
          skipTlsVerify: boolean
        }) => Promise<void>
      }).__embeddedcoworkOpenRemoteWindow
      if (!opener) {
        throw new Error("Remote window opening is not available")
      }
      await opener(payload)
      return { ok: true }
    },
  )

  // --- Update IPC handlers ---
  ipcMain.handle("update:installNow", () => {
    appAutoUpdater?.installNow()
  })

  ipcMain.handle("update:checkNow", () => {
    appAutoUpdater?.checkForUpdates()
  })

  ipcMain.handle("update:rollback", async () => {
    await appAutoUpdater?.rollback()
  })

  // --- Aligned IPC handlers (matching opencode pattern) ---
  ipcMain.handle("check-update", async () => {
    if (!app.isPackaged) return { updateAvailable: false }
    return appAutoUpdater?.checkUpdate() ?? { updateAvailable: false }
  })

  ipcMain.handle("install-update", async () => {
    // 1. Force-stop the CLI server process tree
    await cliManager.forceStop().catch(() => {})

    if (process.platform === "win32") {
      // 2. Kill CLI server by image name (belt-and-suspenders)
      spawnSync("taskkill", ["/F", "/IM", "embeddedcowork-server.exe"], {
        encoding: "utf8",
        timeout: 5000,
      })

      // 3. Get installer path (must use downloadedFile, not the deprecated path)
      const installerPath = appAutoUpdater?.getInstallerPath()
      if (!installerPath) {
        console.error("[install-update] no installer path, falling back to quitAndInstall")
        appAutoUpdater?.installNow()
        return { ok: true as const }
      }

      if (!fs.existsSync(installerPath)) {
        console.error("[install-update] installer not found at", installerPath)
        appAutoUpdater?.installNow()
        return { ok: true as const }
      }

      // 4. Write launcher batch file to temp directory.
      //    The launcher batch immediately kills ALL EmbeddedCowork.exe
      //    processes (including orphan children) via taskkill,
      //    waits ~9 seconds for OS cleanup, then runs the installer.
      //
      //    The batch is spawned DETACHED — it creates a NEW process
      //    group OUTSIDE Electron's Job Object. This is CRITICAL:
      //    if taskkill were spawned from our process (non-detached),
      //    Windows Job Object would terminate it when our process
      //    exits, leaving orphan children alive for the installer to
      //    detect. With detached, the batch and its children survive
      //    our exit and complete the cleanup.
      const tmpDir = app.getPath("temp")
      const launcherPath = `${tmpDir}\\ec-update-${Date.now()}.cmd`
      const batContent =
        `@echo off\r\ntaskkill /f /im EmbeddedCowork.exe > nul 2>&1\r\nping 127.0.0.1 -n 10 > nul\r\n"${installerPath}" --updated\r\ndel "%~f0"\r\n`

      try {
        fs.writeFileSync(launcherPath, batContent, "utf8")

        // 5. Spawn launcher DETACHED — survives our exit
        spawn(launcherPath, [], {
          detached: true,
          stdio: "ignore",
          windowsHide: true,
        }).unref()

        // 6. Exit immediately. The detached batch handles everything:
        //    - Kills all EmbeddedCowork.exe processes (including orphans)
        //    - Waits for OS cleanup (~9 seconds)
        //    - Runs the installer with zero interference
        process.exit(0)
      } catch (err) {
        console.error("[install-update] launcher failed:", err)
        appAutoUpdater?.installNow()
      }

      return { ok: true as const }
    }

    // Non-Windows: use standard quitAndInstall
    appAutoUpdater?.installNow()
    return { ok: true as const }
  })

  ipcMain.handle("get-updater-enabled", () => {
    return app.isPackaged
  })

  ipcMain.handle(
    "notifications:show",
    async (_event, payload: { title?: unknown; body?: unknown }): Promise<{ ok: boolean; reason?: string }> => {
      if (!Notification.isSupported()) {
        return { ok: false, reason: "unsupported" }
      }

      const title = typeof payload?.title === "string" ? payload.title : "EmbeddedCowork"
      const body = typeof payload?.body === "string" ? payload.body : ""
      try {
        const notification = new Notification({ title, body })
        notification.show()
        return { ok: true }
      } catch (error) {
        return { ok: false, reason: error instanceof Error ? error.message : String(error) }
      }
    },
  )
}
