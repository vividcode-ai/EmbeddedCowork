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

export function UpdateNotification() {
  const { t } = useI18n()
  const [showInstallDialog, setShowInstallDialog] = createSignal(false)
  const [activeToastId, setActiveToastId] = createSignal<string | null>(null)

  // Set up platform-specific listeners
  createEffect(() => {
    const host = runtimeEnv.host

    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      if (!api) return

      const unlisteners: Array<() => void> = []

      unlisteners.push(
        api.onUpdateStatus((status: string) => {
          setUpdateStatus(status)
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

          // Show or update downloading toast
          const state = updateState()
          if (state.status === "downloading" && !state.dismissedToast) {
            const percent = Math.round(progress.percent)
            const toast = showToastNotification({
              title: t("update.downloading", { version: state.version ?? "", percent: String(percent) }),
              message: `${formatBytes(progress.transferred)} / ${formatBytes(progress.total)}`,
              variant: "info",
              duration: Number.POSITIVE_INFINITY,
              position: "bottom-right",
            })
            setActiveToastId(toast.id)
            setUpdateProgress({ ...progress, dismissedToast: false })
          }
        }),
      )

      unlisteners.push(
        api.onUpdateReady((info: { version: string }) => {
          readyForInstall(info.version)
          // Dismiss download toast
          if (activeToastId()) {
            // Toast auto-dismisses; we show the dialog instead
            setActiveToastId(null)
          }
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
      // Tauri: use @tauri-apps/plugin-updater JS API
      // Auto-check on startup is handled by Rust side
      initTauriUpdater()
    }
  })

  async function initTauriUpdater() {
    try {
      const { checkUpdate, onUpdaterEvent } = await import("@tauri-apps/plugin-updater")

      // Listen for updater events
      const unlisten = await onUpdaterEvent((event) => {
        switch (event.event) {
          case "CHECKING":
            setUpdateStatus("checking")
            break
          case "UPDATE_AVAILABLE":
            setUpdateStatus("checking") // will transition to downloading when auto-download starts
            break
          case "DOWNLOAD_PROGRESS":
            setUpdateProgress({
              percent: event.data?.progress ?? 0,
              bytesPerSecond: 0,
              total: event.data?.total ?? 0,
              transferred: event.data?.transferred ?? 0,
            })
            break
          case "DOWNLOADED":
            readyForInstall(event.data?.version ?? "")
            setShowInstallDialog(true)
            break
          case "ERROR":
            log.error("Tauri update error:", event.data?.error ?? "unknown")
            showToastNotification({
              title: t("update.error", { error: event.data?.error ?? "Unknown" }),
              message: "",
              variant: "error",
              duration: 8000,
              position: "bottom-right",
            })
            break
        }
      })

      onCleanup(() => {
        try { unlisten() } catch { /* ignore */ }
      })

      // Check for updates
      const update = await checkUpdate()
      if (update?.shouldUpdate) {
        log.info("Tauri update available:", update.manifest?.version)
      }
    } catch (err) {
      log.warn("Tauri updater not available:", err)
    }
  }

  async function handleInstall() {
    const host = runtimeEnv.host
    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      await api?.installUpdate()
    } else if (host === "tauri") {
      try {
        const { installUpdate } = await import("@tauri-apps/plugin-updater")
        await installUpdate()
      } catch (err) {
        log.error("Tauri install update failed:", err)
      }
    }
    setShowInstallDialog(false)
  }

  function handleLater() {
    setShowInstallDialog(false)
  }

  // Calculate human-readable download speed
  function formatBytes(bytes: number): string {
    if (bytes === 0) return "0 B"
    const k = 1024
    const sizes = ["B", "KB", "MB", "GB"]
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
  }

  return (
    <>
      {/* Install dialog */}
      <Show when={showInstallDialog()}>
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/50">
          <div class="w-full max-w-md rounded-lg border border-base bg-surface-primary p-6 shadow-2xl">
            <h2 class="text-lg font-semibold text-primary">{t("update.ready.title")}</h2>
            <p class="mt-2 text-sm text-secondary">
              {t("update.ready.message", { version: updateState().version ?? "" })}
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
