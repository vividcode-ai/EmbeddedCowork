import { Show, createEffect, createSignal, onCleanup } from "solid-js"
import { useI18n } from "../lib/i18n"
import { isElectronHost, isTauriHost, runtimeEnv } from "../lib/runtime-env"
import { showToastNotification } from "../lib/notifications"
import { getLogger } from "../lib/logger"

const log = getLogger("actions")

interface RollbackInfo {
  oldVersion: string
  newVersion: string
}

export function RollbackDialog() {
  const { t } = useI18n()
  const [rollbackInfo, setRollbackInfo] = createSignal<RollbackInfo | null>(null)
  const [isRollingBack, setIsRollingBack] = createSignal(false)

  createEffect(() => {
    const host = runtimeEnv.host

    if (host === "electron") {
      const api = (window as any).electronAPI as ElectronAPI | undefined
      if (!api) return

      const unlisten = api.onRollbackNeeded((data) => {
        setRollbackInfo({ oldVersion: data.oldVersion, newVersion: data.newVersion })
      })

      onCleanup(() => {
        try { unlisten() } catch { /* ignore */ }
      })
    }

    if (host === "tauri") {
      // Tauri: listen for rollback event emitted from Rust
      const tauriBridge = (window as any).__TAURI__
      if (tauriBridge?.event) {
        tauriBridge.event.listen("rollback:needed", (event: { payload: RollbackInfo }) => {
          setRollbackInfo(event.payload)
        }).then((unlisten: () => void) => {
          onCleanup(() => {
            try { unlisten() } catch { /* ignore */ }
          })
        }).catch(() => {})
      }
    }
  })

  async function handleRollback() {
    setIsRollingBack(true)
    try {
      const host = runtimeEnv.host
      if (host === "electron") {
        const api = (window as any).electronAPI as ElectronAPI | undefined
        await api?.rollbackUpdate()
      } else if (host === "tauri") {
        const { invoke } = await import("@tauri-apps/api/core")
        await invoke("rollback_update")
      }
      setRollbackInfo(null)
      showToastNotification({
        title: t("rollback.success", { version: rollbackInfo()?.oldVersion ?? "" }),
        message: "",
        variant: "success",
        duration: 5000,
        position: "bottom-right",
      })
    } catch (err) {
      log.error("Rollback failed:", err)
      showToastNotification({
        title: t("rollback.failed", { error: err instanceof Error ? err.message : String(err) }),
        message: "",
        variant: "error",
        duration: 8000,
        position: "bottom-right",
      })
    } finally {
      setIsRollingBack(false)
    }
  }

  function handleDismiss() {
    setRollbackInfo(null)
  }

  return (
    <Show when={rollbackInfo()}>
      {(info) => (
        <div class="fixed inset-0 z-[110] flex items-center justify-center bg-black/50">
          <div class="w-full max-w-md rounded-lg border border-amber-700 bg-amber-950/90 p-6 shadow-2xl">
            <h2 class="text-lg font-semibold text-amber-100">{t("rollback.title")}</h2>
            <p class="mt-2 text-sm text-amber-200/80">
              {t("rollback.message", {
                oldVersion: info().oldVersion,
                newVersion: info().newVersion,
              })}
            </p>
            <div class="mt-6 flex justify-end gap-3">
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleDismiss}
                disabled={isRollingBack()}
              >
                {t("rollback.dismiss", { version: info().newVersion })}
              </button>
              <button
                type="button"
                class="rounded-lg bg-amber-600 px-4 py-2 text-sm font-semibold text-white hover:bg-amber-500 disabled:opacity-50"
                onClick={handleRollback}
                disabled={isRollingBack()}
              >
                {isRollingBack()
                  ? t("rollback.inProgress", { version: info().oldVersion })
                  : t("rollback.action", { version: info().oldVersion })
                }
              </button>
            </div>
          </div>
        </div>
      )}
    </Show>
  )
}

interface ElectronAPI {
  onRollbackNeeded: (cb: (data: { oldVersion: string; newVersion: string }) => void) => () => void
  rollbackUpdate: () => Promise<void>
}
