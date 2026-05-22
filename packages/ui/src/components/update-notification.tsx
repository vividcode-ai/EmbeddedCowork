import { Show, createEffect, createSignal, onCleanup } from "solid-js"
import { useI18n } from "../lib/i18n"
import { isElectronHost, isTauriHost, runtimeEnv } from "../lib/runtime-env"
import { showToastNotification } from "../lib/notifications"
import {
  updateState,
  setUpdateStatus,
  setUpdateProgress,
  readyForInstall,
  clearUpdateError,
} from "../stores/releases"
import { getLogger } from "../lib/logger"

const log = getLogger("actions")

interface TauriUpdate {
  version: string
  downloadAndInstall(cb?: (event: any) => void): Promise<void>
}

let pendingTauriUpdate: TauriUpdate | null = null

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B"
  const k = 1024
  const sizes = ["B", "KB", "MB", "GB"]
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
}

export function UpdateNotification() {
  const { t } = useI18n()
  const [showInstallDialog, setShowInstallDialog] = createSignal(false)

  // Set up platform-specific listeners
  createEffect(() => {
    const host = runtimeEnv.host

    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      if (!api) return

      const unlisteners: Array<() => void> = []

      unlisteners.push(
        api.onUpdateStatus((status: string) => {
          setUpdateStatus(status as any)
        }),
      )

      unlisteners.push(
        api.onUpdateAvailable((info: { version: string }) => {
          log.info("Update available:", info.version)
        }),
      )

      unlisteners.push(
        api.onUpdateProgress((progress: { percent: number; bytesPerSecond: number; total: number; transferred: number }) => {
          setUpdateProgress(progress)
        }),
      )

      unlisteners.push(
        api.onUpdateReady((info: { version: string }) => {
          readyForInstall(info.version)
          setShowInstallDialog(true)
          log.info("Update ready:", info.version)
        }),
      )

      unlisteners.push(
        api.onUpdateError((err: { message: string }) => {
          log.error("Update error:", err.message)
          clearUpdateError()
          showToastNotification({
            title: t("update.error", { error: err.message }),
            message: "",
            variant: "error",
            duration: 8000,
            position: "bottom-right",
          })
        }),
      )

      onCleanup(() => {
        for (const unlisten of unlisteners) {
          try { unlisten() } catch { /* ignore */ }
        }
      })
    }

    if (host === "tauri") {
      initTauriUpdater()
    }
  })

  async function initTauriUpdater() {
    try {
      const { check } = await import("@tauri-apps/plugin-updater")
      const update = await check()
      if (update) {
        log.info("Tauri update available:", update.version)
        pendingTauriUpdate = update as unknown as TauriUpdate
        setShowInstallDialog(true)
      }
    } catch (err) {
      log.warn("Tauri updater not available:", err)
      showToastNotification({
        title: t("update.checkFailed"),
        message: String(err),
        variant: "error",
        duration: 8000,
        position: "bottom-right",
      })
    }
  }

  async function handleInstall() {
    const host = runtimeEnv.host
    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      await api?.installUpdate()
    } else if (host === "tauri") {
      try {
        let update = pendingTauriUpdate
        pendingTauriUpdate = null
        if (!update) {
          log.warn("Tauri install: no pending update, re-checking")
          const { check } = await import("@tauri-apps/plugin-updater")
          const fresh = await check()
          if (!fresh) return
          update = fresh as unknown as TauriUpdate
        }
        setUpdateStatus("downloading")
        let total = 0
        let transferred = 0
        await update.downloadAndInstall((event) => {
          if (event.event === 'Started') {
            total = event.data.contentLength ?? 0
            transferred = 0
            setUpdateProgress({ percent: 0, bytesPerSecond: 0, total, transferred })
          } else if (event.event === 'Progress') {
            transferred += event.data.chunkLength
            const percent = total > 0 ? Math.round((transferred / total) * 100) : 0
            setUpdateProgress({ percent, bytesPerSecond: 0, total, transferred })
          }
        })
        const { invoke } = await import("@tauri-apps/api/core")
        readyForInstall(update.version)
        await invoke("restart_app")
      } catch (err) {
        log.error("Tauri install update failed:", err)
        setUpdateStatus("error")
        showToastNotification({
          title: t("update.checkFailed"),
          message: String(err),
          variant: "error",
          duration: 8000,
          position: "bottom-right",
        })
      }
    }
    setShowInstallDialog(false)
  }

  function handleLater() {
    setShowInstallDialog(false)
  }

  const state = updateState()
  const progress = state.progress

  return (
    <>
      {/* Download progress bar (fixed bottom-right) */}
      <Show when={state.status === "downloading" && progress && progress.total > 0}>
        <div class="fixed bottom-4 right-4 z-50 flex items-center gap-3 rounded-lg border border-base bg-surface-primary px-4 py-3 shadow-xl min-w-[240px]">
          <div class="flex-1 min-w-0">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs text-secondary truncate">
                {t("update.downloading", {
                  version: state.version ?? "",
                  percent: String(Math.round(progress!.percent)),
                })}
              </span>
              <span class="text-xs font-medium text-primary ml-2">
                {Math.round(progress!.percent)}%
              </span>
            </div>
            <div class="w-full h-1.5 bg-base rounded-full overflow-hidden">
              <div
                class="h-full bg-accent rounded-full transition-all duration-300 ease-out"
                style={{ width: `${progress!.percent}%` }}
              />
            </div>
            <div class="text-[10px] text-muted mt-0.5">
              {formatBytes(progress!.transferred)} / {formatBytes(progress!.total)}
            </div>
          </div>
        </div>
      </Show>

      {/* Install dialog */}
      <Show when={showInstallDialog()}>
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/50">
          <div class="w-full max-w-md rounded-lg border border-base bg-surface-primary p-6 shadow-2xl">
            <h2 class="text-lg font-semibold text-primary">{t("update.ready.title")}</h2>
            <p class="mt-2 text-sm text-secondary">
              {t("update.ready.message", { version: state.version ?? "" })}
            </p>
            <div class="mt-6 flex justify-end gap-3">
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleLater}
              >
                {t("update.ready.dismiss")}
              </button>
              <button
                type="button"
                class="selector-button selector-button-primary"
                onClick={handleInstall}
              >
                {t("update.ready.action")}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </>
  )
}

/** Typed wrapper for window.electronAPI */
interface ElectronAPI {
  onUpdateStatus: (cb: (status: string) => void) => () => void
  onUpdateAvailable: (cb: (info: { version: string }) => void) => () => void
  onUpdateProgress: (cb: (progress: { percent: number; bytesPerSecond: number; total: number; transferred: number }) => void) => () => void
  onUpdateReady: (cb: (info: { version: string }) => void) => () => void
  onUpdateError: (cb: (err: { message: string }) => void) => () => void
  onRollbackNeeded: (cb: (data: { oldVersion: string; newVersion: string }) => void) => () => void
  checkForUpdates: () => Promise<void>
  installUpdate: () => Promise<void>
  rollbackUpdate: () => Promise<void>
}
