import { Component, onMount, onCleanup, createMemo } from "solid-js"
import { useI18n } from "../lib/i18n"
import { startOpencodePolling, stopOpencodePolling, opencodeDownloadPhase, opencodeDownloadProgress } from "../stores/instances"

export const OpencodeSetupScreen: Component = () => {
  const { t } = useI18n()

  onMount(() => {
    startOpencodePolling()
  })

  onCleanup(() => {
    stopOpencodePolling()
  })

  const progressPercent = createMemo(() => {
    const p = opencodeDownloadProgress()
    if (!p || p.total <= 0) return undefined
    return Math.round((p.current / p.total) * 100)
  })

  const statusText = createMemo(() => {
    const phase = opencodeDownloadPhase()
    switch (phase) {
      case "extracting":
        return t("downloader.status.extracting")
      case "verifying":
        return t("downloader.status.verifying")
      case "error":
        return t("downloader.status.error")
      default:
        if (progressPercent() !== undefined) {
          return t("downloader.status.downloading", { progress: progressPercent() })
        }
        return t("downloader.status.downloading", { progress: 0 })
    }
  })

  return (
    <div class="flex h-screen w-full items-center justify-center overflow-hidden py-6 px-4 sm:px-6 relative" style="background-color: var(--surface-secondary)">
      <div class="w-full max-w-md flex flex-col items-center gap-6 text-center">
        <div class="w-16 h-16 rounded-xl flex items-center justify-center" style="background: var(--surface-base); box-shadow: 0 4px 16px rgba(0,0,0,0.08);">
          <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
            <rect width="32" height="32" rx="6" fill="var(--accent-primary)" />
            <path d="M10 16L14 20L22 12" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </div>

        <div class="flex flex-col gap-2">
          <h1 class="text-xl font-semibold" style="color: var(--text-primary)">
            {t("downloader.title")}
          </h1>
          <p class="text-sm" style="color: var(--text-secondary); max-width: 320px; line-height: 1.5;">
            {t("downloader.description")}
          </p>
        </div>

        <div class="w-full" style="max-width: 280px;">
          <div class="w-full h-1 rounded-full overflow-hidden" style="background: var(--border-base);">
            <div class="h-full rounded-full" style="width: 30%; background: var(--accent-primary); animation: opencode-download-progress 1.5s ease-in-out infinite;" />
          </div>
        </div>

        <p class="text-xs" style="color: var(--text-tertiary);">
          {statusText()}
        </p>
      </div>
    </div>
  )
}
