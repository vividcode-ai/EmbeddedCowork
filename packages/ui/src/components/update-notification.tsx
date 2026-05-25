import { Show, createEffect, createSignal, onCleanup } from "solid-js"
import { useI18n } from "../lib/i18n"
import { isElectronHost, isTauriHost, runtimeEnv } from "../lib/runtime-env"
import { showToastNotification } from "../lib/notifications"
import {
  updateState,
  setUpdateState,
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

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.ceil(seconds)}s`
  const m = Math.floor(seconds / 60)
  const s = Math.ceil(seconds % 60)
  return `${m}m ${s}s`
}

interface UpdateProgressPayload {
  status: string
  percent?: number
  total?: number
  downloaded?: number
}

function StepIndicator(props: { steps: string[]; current: number }) {
  return (
    <div class="flex items-center justify-between mb-5">
      {props.steps.map((label, i) => {
        const isComplete = i < props.current
        const isActive = i === props.current
        return (
          <>
            {i > 0 && (
              <div
                class={`flex-1 h-0.5 mx-2 transition-colors duration-300 ${
                  isComplete ? "bg-accent" : "bg-base"
                }`}
              />
            )}
            <div class="flex flex-col items-center gap-1 min-w-0">
              <div
                class={`w-7 h-7 rounded-full flex items-center justify-center text-xs font-bold transition-all duration-300 ${
                  isComplete
                    ? "bg-accent text-white"
                    : isActive
                      ? "bg-accent/20 text-accent border-2 border-accent"
                      : "bg-base text-muted"
                }`}
              >
                {isComplete ? "✓" : i + 1}
              </div>
              <span
                class={`text-[10px] whitespace-nowrap transition-colors duration-300 ${
                  isComplete || isActive ? "text-primary font-medium" : "text-muted"
                }`}
              >
                {label}
              </span>
            </div>
          </>
        )
      })}
    </div>
  )
}

export function UpdateNotification() {
  const { t } = useI18n()
  const [showInstallDialog, setShowInstallDialog] = createSignal(false)
  const [downloadSpeed, setDownloadSpeed] = createSignal(0)
  const [downloadEta, setDownloadEta] = createSignal(0)
  let lastChunkTime = 0
  let lastChunkDownloaded = 0
  let speedSample = 0

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
          if (progress.bytesPerSecond > 0) {
            setDownloadSpeed(progress.bytesPerSecond)
            if (progress.total > 0 && progress.transferred < progress.total) {
              const remaining = progress.total - progress.transferred
              setDownloadEta(remaining / progress.bytesPerSecond)
            }
          }
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

  async function initTauriUpdater() {
    try {
      const { invoke } = await import("@tauri-apps/api/core")
      const info = await invoke<{ version: string; download_url: string } | null>("check_update")
      if (info) {
        log.info("Tauri update available:", info.version)
        setUpdateState((prev) => ({ ...prev, version: info.version, downloadUrl: info.download_url }))
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

    // Store pre-update version for post-update toast
    try {
      const { getVersion } = await import("@tauri-apps/api/app")
      const currentVersion = await getVersion()
      localStorage.setItem("embeddedcowork:preUpdateVersion", currentVersion)
    } catch {
      localStorage.setItem("embeddedcowork:preUpdateVersion", "0.0.0")
    }

    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      await api?.installUpdate()
      setShowInstallDialog(false)
    } else if (host === "tauri") {
      const state = updateState()
      const downloadUrl = state.downloadUrl
      if (!downloadUrl) {
        log.error("No download URL available for Tauri update install")
        setUpdateStatus("error")
        showToastNotification({
          title: t("update.checkFailed"),
          message: "Missing download URL",
          variant: "error",
          duration: 8000,
          position: "bottom-right",
        })
        return
      }

      const { listen } = await import("@tauri-apps/api/event")
      const unlisten = await listen<UpdateProgressPayload>("update:progress", (event) => {
        const p = event.payload
        if (p.status === "downloading") {
          setUpdateStatus("downloading")
          setUpdateProgress({
            percent: p.percent ?? 0,
            bytesPerSecond: 0,
            total: p.total ?? 0,
            transferred: p.downloaded ?? 0,
          })

          // Calculate download speed using time-delta
          const now = Date.now()
          const transferredSinceLast = (p.downloaded ?? 0) - lastChunkDownloaded
          const timeSinceLast = now - lastChunkTime
          if (lastChunkTime > 0 && timeSinceLast > 0) {
            speedSample = transferredSinceLast / (timeSinceLast / 1000)
            speedSample = speedSample > 0 ? speedSample : downloadSpeed()
            setDownloadSpeed(speedSample)
            const remaining = (p.total ?? 0) - (p.downloaded ?? 0)
            if (speedSample > 0) {
              setDownloadEta(remaining / speedSample)
            }
          }
          lastChunkTime = now
          lastChunkDownloaded = p.downloaded ?? 0
        } else if (p.status === "extracting") {
          setUpdateStatus("extracting")
        } else if (p.status === "installing") {
          setUpdateStatus("installing")
        } else if (p.status === "preparing-exit") {
          setUpdateStatus("preparing-exit")
        }
      })
      try {
        const { invoke } = await import("@tauri-apps/api/core")
        setUpdateStatus("downloading")
        await invoke("install_update", { version: state.version, download_url: downloadUrl })
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
      } finally {
        unlisten()
      }
    }
  }

  function handleLater() {
    setShowInstallDialog(false)
  }

  const state = updateState()
  const progress = state.progress

  const stepOrder = (() => {
    switch (state.status) {
      case "downloading":
        return 0
      case "extracting":
        return 1
      case "installing":
      case "preparing-exit":
        return 2
      default:
        return -1
    }
  })()

  const steps = [
    t("update.step.download"),
    t("update.step.prepare"),
    t("update.step.install"),
  ]

  return (
    <>
      <Show when={showInstallDialog() || state.status === "downloading" || state.status === "extracting" || state.status === "installing" || state.status === "preparing-exit" || state.status === "error"}>
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/50">
          <div class="w-full max-w-sm rounded-lg border border-base bg-surface-primary p-6 shadow-2xl">

            {/* Step indicator */}
            <Show when={stepOrder >= 0}>
              <StepIndicator steps={steps} current={stepOrder} />
            </Show>

            <Show when={state.status === "downloading"}>
              <Show when={progress && progress.total > 0} fallback={
                <div class="flex flex-col items-center py-4">
                  <span class="inline-block w-8 h-8 border-[3px] border-accent border-t-transparent rounded-full animate-spin mb-3" />
                  <h2 class="text-lg font-semibold text-primary">
                    {t("update.downloading", { version: state.version ?? "", percent: "0" })}
                  </h2>
                  <p class="mt-2 text-sm text-secondary">{t("update.connecting")}</p>
                </div>
              }>
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
                <div class="mt-2 text-xs text-muted flex justify-between">
                  <span>{formatBytes(progress!.transferred)} / {formatBytes(progress!.total)}</span>
                  <Show when={downloadSpeed() > 0}>
                    <span class="text-accent">
                      {t("update.downloadSpeed", { speed: formatBytes(downloadSpeed()) })}
                      {downloadEta() > 0 && downloadEta() < 3600 && (
                        <> · {t("update.eta", { time: formatDuration(downloadEta()) })}</>
                      )}
                    </span>
                  </Show>
                </div>
              </Show>
            </Show>

            <Show when={state.status === "extracting"}>
              <div class="flex flex-col items-center py-4">
                <span class="inline-block w-8 h-8 border-[3px] border-accent border-t-transparent rounded-full animate-spin mb-3" />
                <h2 class="text-lg font-semibold text-primary">{t("update.extracting")}</h2>
              </div>
            </Show>

            <Show when={state.status === "installing"}>
              <div class="flex flex-col items-center py-4">
                <span class="inline-block w-8 h-8 border-[3px] border-accent border-t-transparent rounded-full animate-spin mb-3" />
                <h2 class="text-lg font-semibold text-primary">{t("update.installing")}</h2>
                <p class="mt-2 text-sm text-secondary text-center">{t("update.installing.message")}</p>
              </div>
            </Show>

            <Show when={state.status === "preparing-exit"}>
              <div class="flex flex-col items-center py-4">
                <span class="inline-block w-8 h-8 border-[3px] border-accent border-t-transparent rounded-full animate-spin mb-3" />
                <h2 class="text-lg font-semibold text-primary whitespace-pre-line text-center">
                  {t("update.preparingExit")}
                </h2>
              </div>
            </Show>

            <Show when={state.status !== "downloading" && state.status !== "extracting" && state.status !== "installing" && state.status !== "preparing-exit" && showInstallDialog()}>
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
