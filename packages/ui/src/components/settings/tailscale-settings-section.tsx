import { Switch } from "@kobalte/core/switch"
import { createMemo, createSignal, onMount, type Component, For, Show } from "solid-js"
import { Copy, ExternalLink, Loader2, Network, RefreshCw, QrCode } from "lucide-solid"
import { toDataURL } from "qrcode"
import { useI18n } from "../../lib/i18n"
import { getLogger } from "../../lib/logger"
import type { TailscaleStatus } from "../../../../server/src/api-types"

async function tailscaleApi(path: string, options?: RequestInit): Promise<any> {
  const base = window.location.origin
  const response = await fetch(`${base}/api/tailscale${path}`, {
    ...options,
    headers: { ...options?.headers, "Content-Type": "application/json" },
    credentials: "include",
  })
  if (!response.ok) {
    throw new Error(`tailscale API error: ${response.status}`)
  }
  return response.json()
}

export const TailscaleSettingsSection: Component = () => {
  const { t } = useI18n()
  const log = getLogger("tailscale")
  const [status, setStatus] = createSignal<TailscaleStatus | null>(null)
  const [loading, setLoading] = createSignal(false)
  const [authKey, setAuthKey] = createSignal("")
  const [authKeySaving, setAuthKeySaving] = createSignal(false)
  const [authKeyError, setAuthKeyError] = createSignal<string | null>(null)
  const [loginUrl, setLoginUrl] = createSignal<string | null>(null)
  const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null)
  const [error, setError] = createSignal<string | null>(null)
  const [controlUrls, setControlUrls] = createSignal<string[]>([])
  const [activeUrl, setActiveUrl] = createSignal("")
  const [newUrl, setNewUrl] = createSignal("")
  const [savingUrls, setSavingUrls] = createSignal(false)
  const [urlsError, setUrlsError] = createSignal<string | null>(null)

  const fetchControlUrls = async () => {
    try {
      const data = await tailscaleApi("/control-urls")
      const urls: string[] = data.urls ?? []
      if (urls.length === 0) {
        urls.push("https://control.tailscale.com")
      }
      setControlUrls(urls)
      setActiveUrl(data.activeUrl || urls[0])
    } catch {
      setControlUrls(["https://control.tailscale.com"])
      setActiveUrl("https://control.tailscale.com")
    }
  }

  const handleAddUrl = () => {
    const url = newUrl().trim()
    if (!url) return
    if (controlUrls().includes(url)) return
    setControlUrls([...controlUrls(), url])
    setNewUrl("")
  }

  const handleRemoveUrl = (url: string) => {
    const next = controlUrls().filter((u) => u !== url)
    setControlUrls(next)
    if (activeUrl() === url) {
      setActiveUrl(next[0] ?? "")
    }
  }

  const handleSaveControlUrls = async () => {
    setSavingUrls(true)
    setUrlsError(null)
    try {
      await tailscaleApi("/control-urls", {
        method: "PUT",
        body: JSON.stringify({ urls: controlUrls(), activeUrl: activeUrl() }),
      })
      await fetchControlUrls()
      await fetchStatus()
    } catch (err) {
      setUrlsError(err instanceof Error ? err.message : String(err))
    } finally {
      setSavingUrls(false)
    }
  }

  const fetchStatus = async () => {
    setLoading(true)
    setError(null)
    try {
      const data: TailscaleStatus = await tailscaleApi("/status")
      setStatus(data)
      if (data.authNeeded && data.loginURL) {
        setLoginUrl(data.loginURL)
      } else if (data.authNeeded) {
        const loginResult = await tailscaleApi("/login-url").catch(() => ({ ok: false }))
        if (loginResult.ok && loginResult.url) {
          setLoginUrl(loginResult.url)
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  onMount(() => {
    void fetchStatus()
    void fetchControlUrls()
  })

  const handleSaveAuthKey = async () => {
    const key = authKey().trim()
    if (!key) return

    setAuthKeySaving(true)
    setAuthKeyError(null)
    try {
      const result = await tailscaleApi("/auth-key", {
        method: "POST",
        body: JSON.stringify({ authKey: key }),
      })
      if (result.ok) {
        setTimeout(() => void fetchStatus(), 3000)
      } else {
        setAuthKeyError(result.error ?? "Unknown error")
      }
    } catch (err) {
      setAuthKeyError(err instanceof Error ? err.message : String(err))
    } finally {
      setAuthKeySaving(false)
    }
  }

  const handleCopyLoginUrl = async () => {
    setQrDataUrl(null)
    const url = loginUrl()
    if (url) {
      try {
        await navigator.clipboard.writeText(url)
      } catch {
        // clipboard not available
      }
    }
  }

  const handleShowQr = async () => {
    const url = loginUrl()
    if (!url) return

    if (qrDataUrl()) {
      setQrDataUrl(null)
      return
    }

    try {
      const dataUrl = await toDataURL(url, { margin: 1, scale: 4 })
      setQrDataUrl(dataUrl)
    } catch (err) {
      log.error("Failed to generate QR code", err)
    }
  }

  const handleOpenLoginUrl = () => {
    setQrDataUrl(null)
    const url = loginUrl()
    if (url) {
      window.open(url, "_blank", "noopener,noreferrer")
    }
  }

  const handleStop = async () => {
    await tailscaleApi("/stop", { method: "POST" })
    await fetchStatus()
  }

  const handleStart = async () => {
    await tailscaleApi("/start", { method: "POST" })
    await fetchStatus()
  }

  const s = createMemo(() => status())

  return (
    <div class="settings-section-stack">
      <Show when={error()}>
        <div class="settings-error-message">
          {t("tailscale.error.overview", { error: error() })}
        </div>
      </Show>

      <div class="settings-card">
        <div class="settings-card-header">
          <div class="settings-card-heading-with-icon">
            <Network class="settings-card-heading-icon" />
            <div>
              <h3 class="settings-card-title">{t("tailscale.title")}</h3>
              <p class="settings-card-subtitle">{t("tailscale.description")}</p>
            </div>
          </div>
        </div>

        <div class="settings-stack">
          <div class="settings-toggle-row">
            <span class="settings-toggle-title">{t("tailscale.status.label")}</span>
            <span class="settings-field-value-inline">
              <Show when={loading()}>
                <Loader2 class="settings-icon-spin" />
              </Show>
              <Show when={!loading() && s()}>
                <span
                  class="settings-status-dot"
                  data-status={s()?.connected ? "connected" : s()?.authNeeded ? "warning" : "disconnected"}
                />
                <Show when={s()?.connected} fallback={
                  <Show when={s()?.authNeeded} fallback={t("tailscale.status.disconnected")}>
                    {t("tailscale.status.authNeeded")}
                  </Show>
                }>
                  {t("tailscale.status.connected")}
                </Show>
              </Show>
              <Show when={!loading() && !s()}>
                {t("tailscale.status.disconnected")}
              </Show>
            </span>
          </div>

          <Show when={!loading() && s()?.error}>
            <div class="settings-error-message">{s()!.error}</div>
          </Show>

          <Show when={s()?.connected}>
            <div class="settings-toggle-row">
              <span class="settings-toggle-title">{t("tailscale.ips")}</span>
              <div class="settings-ip-list">
                {s()!.tailscaleIPs.map((ip) => (
                  <code class="settings-ip-chip">
                    {ip}
                    <button
                      type="button"
                      class="settings-icon-button"
                      onClick={() => navigator.clipboard.writeText(ip)}
                      title={t("tailscale.copy")}
                    >
                      <Copy class="w-3 h-3" />
                    </button>
                  </code>
                ))}
              </div>
            </div>

            <Show when={s()?.hostname}>
              <div class="settings-toggle-row">
                <span class="settings-toggle-title">{t("tailscale.hostname")}</span>
                <span class="settings-field-value-inline">{s()!.hostname}</span>
              </div>
            </Show>
          </Show>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <div class="settings-card-heading-with-icon">
            <Network class="settings-card-heading-icon" />
            <div>
              <h3 class="settings-card-title">{t("tailscale.settings.controlUrls")}</h3>
            </div>
          </div>
        </div>

        <div class="settings-stack">
          <Show when={controlUrls().length === 0}>
            <p class="settings-help-text">{t("tailscale.settings.controlUrls.empty")}</p>
          </Show>

          <For each={controlUrls()}>
            {(url) => (
              <div class="settings-toggle-row">
                <label class="settings-checkbox-toggle">
                  <input
                    type="radio"
                    name="controlUrl"
                    checked={activeUrl() === url}
                    onChange={() => setActiveUrl(url)}
                  />
                  <span class="settings-toggle-title" style="font-size:var(--font-size-xs);word-break:break-all">{url}</span>
                </label>
                <button
                  type="button"
                  class="selector-button selector-button-secondary w-auto"
                  style="width:auto;padding:0.25rem 0.5rem;font-size:var(--font-size-xs)"
                  onClick={() => handleRemoveUrl(url)}
                >
                  {t("tailscale.settings.controlUrls.remove")}
                </button>
              </div>
            )}
          </For>

          <div class="selector-input-group">
            <input
              type="text"
              class="selector-input"
              placeholder={t("tailscale.settings.controlUrls.placeholder")}
              value={newUrl()}
              onInput={(e) => setNewUrl(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && handleAddUrl()}
            />
            <button
              type="button"
              class="selector-button selector-button-secondary w-auto"
              disabled={!newUrl().trim()}
              onClick={handleAddUrl}
            >
              {t("tailscale.settings.controlUrls.add")}
            </button>
          </div>

          <Show when={urlsError()}>
            <p class="settings-error-message">{urlsError()}</p>
          </Show>

          <button
            type="button"
            class="selector-button selector-button-primary"
            disabled={savingUrls() || controlUrls().length === 0}
            onClick={handleSaveControlUrls}
          >
            <Show when={savingUrls()} fallback={t("tailscale.settings.controlUrls.apply")}>
              <Loader2 class="settings-icon-spin" />
              {t("tailscale.settings.controlUrls.saved")}
            </Show>
          </button>
        </div>
      </div>

      <Show when={s()?.authNeeded}>
        <div class="settings-card">
          <h4 class="settings-card-title">{t("tailscale.auth.authKey.label")}</h4>
          <div class="selector-input-group">
            <input
              type="password"
              class="selector-input"
              placeholder={t("tailscale.auth.authKey.placeholder")}
              value={authKey()}
              onInput={(e) => setAuthKey(e.currentTarget.value)}
            />
            <button
              type="button"
              class="selector-button selector-button-primary w-auto"
              disabled={authKeySaving() || !authKey().trim()}
              onClick={handleSaveAuthKey}
            >
              <Show when={authKeySaving()} fallback={t("tailscale.auth.authKey.save")}>
                <Loader2 class="settings-icon-spin" />
              </Show>
            </button>
          </div>
          <Show when={authKeyError()}>
            <p class="settings-error-message">{t("tailscale.auth.authKey.error", { error: authKeyError() })}</p>
          </Show>
        </div>

        <Show when={loginUrl()}>
          <div class="settings-card">
            <h4 class="settings-card-title">{t("tailscale.auth.interactive.label")}</h4>
            <div class="settings-toolbar-inline" style="margin-top: 0.75rem">
              <button
                type="button"
                class="selector-button selector-button-secondary w-auto"
                onClick={handleOpenLoginUrl}
              >
                <ExternalLink class="w-4 h-4" />
                {t("tailscale.auth.interactive.open")}
              </button>
              <button
                type="button"
                class="selector-button selector-button-secondary w-auto"
                onClick={handleCopyLoginUrl}
              >
                <Copy class="w-4 h-4" />
                {t("tailscale.auth.interactive.copy")}
              </button>
              <button
                type="button"
                class="selector-button selector-button-secondary w-auto"
                onClick={handleShowQr}
              >
                <QrCode class="w-4 h-4" />
                {t("tailscale.auth.interactive.qr")}
              </button>
            </div>
            <Show when={qrDataUrl()}>
              <div class="remote-qr" style="margin-top: 0.75rem">
                <img src={qrDataUrl()!} alt="QR Code" class="remote-qr-img" />
              </div>
            </Show>
          </div>
        </Show>
      </Show>

      <div class="settings-card">
        <div class="settings-toolbar-inline" style="justify-content: flex-end">
          <Show when={s()?.connected}>
            <button
              type="button"
              class="selector-button selector-button-danger w-auto"
              onClick={handleStop}
            >
              {t("tailscale.disconnect")}
            </button>
          </Show>
          <button
            type="button"
            class="selector-button selector-button-secondary w-auto"
            onClick={fetchStatus}
            disabled={loading()}
          >
            <RefreshCw class="w-4 h-4" />
            {t("tailscale.refresh")}
          </button>
        </div>
      </div>
    </div>
  )
}
