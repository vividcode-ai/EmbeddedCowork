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

      const tmpDir = app.getPath("temp")
      const bootId = Date.now()
      const taskName = `ECUpdate-${bootId}`
      const helperPath = `${tmpDir}\\ec-update-${bootId}.cmd`
      const logPath = `${tmpDir}\\ec-update-${bootId}.log`

      // 4. Write a helper batch that runs via Windows Task Scheduler.
      //    Unlike spawn(..., {detached:true}) which stays in Electron's
      //    Job Object (CREATE_NEW_PROCESS_GROUP != CREATE_BREAKAWAY_FROM_JOB),
      //    a scheduled task is created by the Task Scheduler service
      //    (svchost.exe) — a completely separate process tree outside
      //    Electron's Job Object. This guarantees the helper survives
      //    our exit and KILL_ON_JOB_CLOSE.
      //
      //    The batch:
      //      - Waits 20s for our process to fully exit
      //      - Kills EmbeddedCowork.exe in a retry loop until none remain
      //      - Runs the NSIS installer
      //      - Cleans up its own task registration and files
      //
      //    The retry loop is CRITICAL: if a process respawns between
      //    taskkill and the installer's FIND_PROCESS (e.g. via Chromium
      //    process management or Windows auto-restart), the loop keeps
      //    killing until the process table is clean before launching
      //    the installer.
      const helperContent = [
        `@echo off`,
        `set LOG="${logPath}"`,
        `echo [%DATE% %TIME%] Helper started >> %LOG%`,
        `REM Wait for the main process to fully exit`,
        `ping 127.0.0.1 -n 20 > nul`,
        `echo [%DATE% %TIME%] Ping done >> %LOG%`,
        `:kill_loop`,
        `taskkill /f /im EmbeddedCowork.exe > nul 2>&1`,
        `taskkill /f /im embeddedcowork-server.exe > nul 2>&1`,
        `REM Small delay to let OS cleanup process handles`,
        `ping 127.0.0.1 -n 3 > nul`,
        `REM Check if any EmbeddedCowork.exe processes remain`,
        `tasklist /FI "IMAGENAME eq EmbeddedCowork.exe" 2>nul | find /i "EmbeddedCowork.exe" > nul`,
        `if not errorlevel 1 goto kill_loop`,
        `echo [%DATE% %TIME%] All processes killed >> %LOG%`,
        `REM Log remaining processes for diagnostics`,
        `echo [%DATE% %TIME%] Final process list: >> %LOG%`,
        `tasklist /FI "IMAGENAME eq EmbeddedCowork.exe" /FO CSV >> %LOG% 2>&1`,
        `tasklist /FI "IMAGENAME eq embeddedcowork-server.exe" /FO CSV >> %LOG% 2>&1`,
        `echo [%DATE% %TIME%] Launching installer: "${installerPath}" --updated >> %LOG%`,
        `REM Run the installer`,
        `"${installerPath}" --updated >> %LOG% 2>&1`,
        `set INSTALLER_EXIT=%ERRORLEVEL%`,
        `echo [%DATE% %TIME%] Installer exited with code %INSTALLER_EXIT% >> %LOG%`,
        `REM If installer failed (non-zero), log it but still clean up`,
        `if %INSTALLER_EXIT% neq 0 echo [%DATE% %TIME%] INSTALLER FAILED >> %LOG%`,
        `REM Clean up task registration`,
        `schtasks /delete /tn "${taskName}" /f > nul 2>&1`,
        `echo [%DATE% %TIME%] Cleanup done >> %LOG%`,
        `del "%~f0"`,
      ].join("\r\n")

      try {
        // Log pre-kill process list for diagnostics
        const preKillTaskList = spawnSync("tasklist", ["/FI", "IMAGENAME eq EmbeddedCowork.exe", "/FO", "CSV"], {
          encoding: "utf8",
          timeout: 5000,
        })
        console.log("[install-update] pre-kill EmbeddedCowork.exe processes:", preKillTaskList.stdout)

        fs.writeFileSync(helperPath, helperContent, "utf8")

        // 5. Create a one-shot scheduled task.
        //    /ru "" = current user, /rl limited = limited rights,
        //    /f = force (no confirmation prompt).
        const createResult = spawnSync("schtasks", [
          "/create", "/tn", taskName,
          "/tr", `"${helperPath}"`,
          "/sc", "once", "/st", "00:00",
          "/f",
          "/rl", "limited",
          "/ru", "",
        ], { encoding: "utf8", timeout: 10000 })

        if (createResult.status !== 0) {
          // schtasks might fail if the user lacks permission (group
          // policy, managed device). Fall back to the standard
          // electron-updater approach.
          console.error("[install-update] schtasks /create failed:", createResult.stderr)
          appAutoUpdater?.installNow()
          return { ok: true as const }
        }

        console.log("[install-update] schtasks /create succeeded")

        // 6. Run the task immediately. The helper process is spawned by
        //    Task Scheduler (svchost.exe), OUTSIDE Electron's Job Object.
        const runResult = spawnSync("schtasks", ["/run", "/tn", taskName], {
          encoding: "utf8",
          timeout: 10000,
        })
        if (runResult.status !== 0) {
          console.error("[install-update] schtasks /run failed:", runResult.stderr)
        } else {
          console.log("[install-update] schtasks /run succeeded")
        }

        // 7. Delete the task registration. The running instance is
        //    independent and continues in the background.
        spawnSync("schtasks", ["/delete", "/tn", taskName, "/f"], {
          encoding: "utf8",
          timeout: 5000,
        })

        // 8. Kill all EmbeddedCowork.exe in-process immediately so the
        //    helper won't find any survivors when it wakes up.
        spawnSync("taskkill", ["/F", "/IM", "EmbeddedCowork.exe"], {
          encoding: "utf8",
          timeout: 5000,
        })

        // 9. Exit. The scheduled helper is outside the Job Object and
        //    WILL survive this call.
        app.exit(0)
      } catch (err) {
        console.error("[install-update] failed:", err)
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
