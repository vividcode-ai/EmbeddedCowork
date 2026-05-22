import { Show, createEffect, createSignal, onCleanup } from "solid-js"
import { useI18n } from "../lib/i18n"
import { isElectronHost, isTauriHost, runtimeEnv } from "../lib/runtime-env"
import { showToastNotification } from "../lib/notifications"
import {
  updateState,
  setUpdateStatus,
  setUpdateProgress,
  clearUpdateError,
} from "../stores/releases"
import { getLogger } from "../lib/logger"

const log = getLogger("actions")

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B"
  const k = 1024
  const sizes = ["B", "KB", "MB", "GB"]
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`
}

interface UpdateProgressPayload {
  status: string
  percent?: number
  total?: number
  downloaded?: number
}

export function UpdateNotification() {
  const { t } = useI18n()
  const [showInstallDialog, setShowInstallDialog] = createSignal(false)

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
          setUpdateStatus("ready")
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

  if (runtimeEnv.host === "tauri") {
    createEffect(() => {
      let unlisten: (() => void) | undefined
      ;(async () => {
        const { listen } = await import("@tauri-apps/api/event")
        const unlistenFn = await listen<UpdateProgressPayload>("update:progress", (event) => {
          const p = event.payload
          if (p.status === "downloading") {
            setUpdateStatus("downloading")
            setUpdateProgress({
              percent: p.percent ?? 0,
              bytesPerSecond: 0,
              total: p.total ?? 0,
              transferred: p.downloaded ?? 0,
            })
          } else if (p.status === "extracting") {
            setUpdateStatus("extracting")
          } else if (p.status === "installing") {
            setUpdateStatus("installing")
          }
        })
        unlisten = unlistenFn
      })()
      onCleanup(() => {
        unlisten?.()
      })
    })
  }

  async function initTauriUpdater() {
    try {
      const { check } = await import("@tauri-apps/plugin-updater")
      const update = await check()
      if (update) {
        log.info("Tauri update available:", update.version)
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
      setShowInstallDialog(false)
    } else if (host === "tauri") {
      try {
        const { invoke } = await import("@tauri-apps/api/core")
        setUpdateStatus("downloading")
        setShowInstallDialog(false)
        await invoke("install_update")
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
  }

  function handleLater() {
    setShowInstallDialog(false)
  }

  const state = updateState()
  const progress = state.progress

  return (
    <>
      <Show when={showInstallDialog() || state.status === "downloading" || state.status === "extracting" || state.status === "installing"}>
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/50">
          <div class="w-full max-w-sm rounded-lg border border-base bg-surface-primary p-6 shadow-2xl">

            <Show when={state.status === "downloading" && progress && progress.total > 0}>
              <h2 class="text-lg font-semibold text-primary mb-3">
                {t("update.downloading", { version: state.version ?? "", percent: String(Math.round(progress!.percent)) })}
              </h2>
              <div class="w-full h-2 bg-base rounded-full overflow-hidden">
                <div
                  class="h-full bg-accent rounded-full transition-all duration-300 ease-out"
                  style={{ width: `${progress!.percent}%` }}
                />
              </div>
              <p class="mt-1 text-xs text-muted text-right">{Math.round(progress!.percent)}%</p>
              <div class="mt-2 text-[10px] text-muted">
                {formatBytes(progress!.transferred)} / {formatBytes(progress!.total)}
              </div>
            </Show>

            <Show when={state.status === "extracting"}>
              <h2 class="text-lg font-semibold text-primary">{t("update.extracting")}</h2>
              <div class="mt-3 flex items-center gap-2 text-sm text-secondary">
                <span class="inline-block w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin" />
                {t("update.extracting")}
              </div>
            </Show>

            <Show when={state.status === "installing"}>
              <h2 class="text-lg font-semibold text-primary">{t("update.installing")}</h2>
              <p class="mt-2 text-sm text-secondary">{t("update.installing.message")}</p>
              <div class="mt-3 flex items-center gap-2 text-sm text-secondary">
                <span class="inline-block w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin" />
                {t("update.installing")}
              </div>
            </Show>

            <Show when={state.status !== "downloading" && state.status !== "extracting" && state.status !== "installing" && showInstallDialog()}>
              <h2 class="text-lg font-semibold text-primary">{t("update.ready.title")}</h2>
              <p class="mt-2 text-sm text-secondary">{t("update.ready.message", { version: state.version ?? "" })}</p>
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
            </Show>

          </div>
        </div>
      </Show>
    </>
  )
}

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
