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

    if (process.platform !== "win32") {
      appAutoUpdater?.installNow()
      return { ok: true as const }
    }

    // ------------------------------------------------------------------
    // Windows update strategy:
    //
    // Problem: Electron's app.exit(0) triggers KILL_ON_JOB_CLOSE via the
    // Windows Job Object in which Chromium places every child process.
    // spawn(..., {detached:true}) uses CREATE_NEW_PROCESS_GROUP (0x200)
    // but NOT CREATE_BREAKAWAY_FROM_JOB (0x02000000), so detached children
    // still die when Electron exits.  This means the NSIS installer gets
    // killed before it can finish.
    //
    // Solution: Launch a helper (or the installer) via an API that runs
    // it OUTSIDE Electron's Job Object entirely.  We try a chain of
    // increasingly aggressive strategies:
    //
    //   A) PowerShell CIM + helper batch  ← primary (safe, reliable)
    //   B) schtasks + helper batch        ← fallback 1
    //   C) PowerShell CIM → direct        ← fallback 2 (no delay)
    //   D) electron-updater quitAndInstall ← last resort
    //
    // The helper batch (A & B) waits 20s for the process to fully exit,
    // then kills EmbeddedCowork.exe in a retry loop before launching the
    // installer.  Fallback C skips the delay — it relies on the NSIS
    // installer's built-in 8-second retry loop giving our process enough
    // time to exit.
    // ------------------------------------------------------------------

    // 2. Kill CLI server by image name (belt-and-suspenders)
    spawnSync("taskkill", ["/F", "/IM", "embeddedcowork-server.exe"], {
      encoding: "utf8",
      timeout: 5000,
    })

    // 3. Get installer path
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
    const helperPath = `${tmpDir}\\ec-update-${bootId}.cmd`
    const logPath = `${tmpDir}\\ec-update-${bootId}.log`

    // 4. Write the helper batch.
    //
    //    The batch waits 20s (process exit grace period), then enters a
    //    kill loop that repeatedly terminates EmbeddedCowork.exe until
    //    none remain (up to 60 retries ≈ 3 minutes).  This handles the
    //    case where Chromium respawns helper processes before the NSIS
    //    installer's FIND_PROCESS check runs.
    //
    //    After cleanup, it launches the installer with --updated.
    const helperContent = [
      `@echo off`,
      `set LOG="${logPath}"`,
      `echo [%DATE% %TIME%] Helper started >> %LOG%`,
      `REM Wait for the main process to fully exit`,
      `ping 127.0.0.1 -n 20 > nul`,
      `echo [%DATE% %TIME%] Ping done >> %LOG%`,
      `REM Kill loop with retry limit (60 retries ~= 3 min)`,
      `set KILL_RETRIES=0`,
      `:kill_loop`,
      `if %KILL_RETRIES% geq 60 goto kill_done`,
      `taskkill /f /im EmbeddedCowork.exe > nul 2>&1`,
      `taskkill /f /im embeddedcowork-server.exe > nul 2>&1`,
      `REM Small delay to let OS cleanup process handles`,
      `ping 127.0.0.1 -n 3 > nul`,
      `REM Check if any EmbeddedCowork.exe processes remain`,
      `tasklist /FI "IMAGENAME eq EmbeddedCowork.exe" 2>nul | find /i "EmbeddedCowork.exe" > nul`,
      `if errorlevel 1 goto kill_done`,
      `set /a KILL_RETRIES=%KILL_RETRIES% + 1`,
      `goto kill_loop`,
      `:kill_done`,
      `echo [%DATE% %TIME%] Kill loop ended after %KILL_RETRIES% retries >> %LOG%`,
      `REM Log remaining processes for diagnostics`,
      `echo [%DATE% %TIME%] Final process list: >> %LOG%`,
      `tasklist /FI "IMAGENAME eq EmbeddedCowork.exe" /FO CSV >> %LOG% 2>&1`,
      `tasklist /FI "IMAGENAME eq embeddedcowork-server.exe" /FO CSV >> %LOG% 2>&1`,
      `echo [%DATE% %TIME%] Launching installer: "${installerPath}" --updated >> %LOG%`,
      `REM Run the installer`,
      `"${installerPath}" --updated >> %LOG% 2>&1`,
      `set INSTALLER_EXIT=%ERRORLEVEL%`,
      `echo [%DATE% %TIME%] Installer exited with code %INSTALLER_EXIT% >> %LOG%`,
      `if %INSTALLER_EXIT% neq 0 echo [%DATE% %TIME%] INSTALLER FAILED >> %LOG%`,
      `echo [%DATE% %TIME%] Cleanup done >> %LOG%`,
      `del "%~f0"`,
    ].join("\r\n")

    // Log pre-kill process list for diagnostics
    try {
      const preKillTaskList = spawnSync("tasklist", ["/FI", "IMAGENAME eq EmbeddedCowork.exe", "/FO", "CSV"], {
        encoding: "utf8",
        timeout: 5000,
      })
      console.log("[install-update] pre-kill EmbeddedCowork.exe processes:", preKillTaskList.stdout)
    } catch { /* ignore */ }

    fs.writeFileSync(helperPath, helperContent, "utf8")

    // ------------------------------------------------------------------
    // Escape strategies — each tries to run something outside the Job
    // Object.  Returned strings are log messages; null means failure.
    // ------------------------------------------------------------------
    const escapedHelperPaths: Array<{ try: () => string | null; label: string }> = []

    // --- A) PowerShell CIM → helper batch ---
    escapedHelperPaths.push({
      label: "PowerShell CIM",
      try(): string | null {
        const psCmd =
          `(Invoke-CimMethod -ClassName Win32_Process -MethodName Create ` +
          `-Arguments @{CommandLine='cmd.exe /c "${helperPath}"'}).ReturnValue`
        const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", psCmd], {
          encoding: "utf8",
          timeout: 15000,
        })
        if (result.status !== 0) {
          console.error("[install-update] PowerShell CIM helper launch failed:", result.stderr)
          return null
        }
        const returnValue = (result.stdout ?? "").trim()
        if (returnValue !== "0") {
          console.error("[install-update] PowerShell CIM helper launch returned (stdout):", result.stdout)
          return null
        }
        return returnValue
      },
    })

    // --- B) schtasks → helper batch ---
    escapedHelperPaths.push({
      label: "schtasks",
      try(): string | null {
        const taskName = `ECUpdate-${bootId}`
        const createResult = spawnSync("schtasks", [
          "/create", "/tn", taskName,
          "/tr", `${helperPath}`,
          "/sc", "once", "/st", "00:00",
          "/f", "/rl", "limited",
        ], { encoding: "utf8", timeout: 10000 })
        if (createResult.status !== 0) {
          console.error("[install-update] schtasks /create failed:", createResult.stderr)
          return null
        }
        const runResult = spawnSync("schtasks", ["/run", "/tn", taskName], {
          encoding: "utf8", timeout: 10000,
        })
        spawnSync("schtasks", ["/delete", "/tn", taskName, "/f"], { timeout: 5000 })
        if (runResult.status !== 0) {
          console.error("[install-update] schtasks /run failed:", runResult.stderr)
          return null
        }
        return runResult.stdout?.trim() || "ok"
      },
    })

    // --- C) PowerShell CIM → direct installer (no delay) ---
    escapedHelperPaths.push({
      label: "CIM-direct",
      try(): string | null {
        // The installer's own _CHECK_APP_RUNNING macro retries for ~8 s
        // before showing the "can't close" dialog.  Our process should
        // exit within 1–2 s, well within that window.
        const psCmd =
          `(Invoke-CimMethod -ClassName Win32_Process -MethodName Create ` +
          `-Arguments @{CommandLine='"${installerPath}" --updated'}).ReturnValue`
        const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", psCmd], {
          encoding: "utf8", timeout: 15000,
        })
        if (result.status !== 0) {
          console.error("[install-update] CIM-direct launch failed:", result.stderr)
          return null
        }
        const returnValue = (result.stdout ?? "").trim()
        if (returnValue !== "0") {
          console.error("[install-update] CIM-direct launch returned (stdout):", result.stdout)
          return null
        }
        return returnValue
      },
    })

    // 5. Try each escape strategy in order.  Keep track of the first
    //    success so we can log it.
    let launched = false
    for (const strategy of escapedHelperPaths) {
      try {
        const result = strategy.try()
        if (result !== null) {
          console.log(`[install-update] ${strategy.label} succeeded:`, result)
          launched = true
          break
        }
        console.warn(`[install-update] ${strategy.label} failed`)
      } catch (err) {
        console.error(`[install-update] ${strategy.label} threw:`, err)
      }
    }

    if (!launched) {
      console.error("[install-update] all escape strategies failed, falling back to quitAndInstall")
      appAutoUpdater?.installNow()
      return { ok: true as const }
    }

    console.log("[install-update] escape launched, killing processes and exiting")

    // 6. Kill all EmbeddedCowork.exe so no orphan survives to the
    //    installer's CHECK_APP_RUNNING scan.
    spawnSync("taskkill", ["/F", "/IM", "EmbeddedCowork.exe"], {
      encoding: "utf8",
      timeout: 5000,
    })

    // 7. Exit.  The helper/installer is outside the Job Object and
    //    WILL survive this call.
    app.exit(0)

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
