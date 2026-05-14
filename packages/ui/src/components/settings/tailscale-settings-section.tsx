import { Switch } from "@kobalte/core/switch"
import { createSignal, onMount, type Component, Show } from "solid-js"
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

  const s = status()

  return (
    <div class="settings-section">
      <div class="settings-section-header">
        <Network class="settings-section-header-icon" />
        <div>
          <h3 class="settings-section-title">{t("tailscale.title")}</h3>
          <p class="settings-section-description">{t("tailscale.description")}</p>
        </div>
      </div>

      <Show when={error()}>
        <div class="settings-error">
          {t("tailscale.error.overview", { error: error() })}
        </div>
      </Show>

      <div class="settings-card">
        <div class="settings-field-row">
          <span class="settings-field-label">{t("tailscale.status.label")}</span>
          <span class="settings-field-value">
            <Show when={loading()}>
              <Loader2 class="settings-icon-spin" />
            </Show>
            <Show when={!loading() && s}>
              <span
                class="settings-status-dot"
                data-status={s?.connected ? "connected" : s?.authNeeded ? "warning" : "disconnected"}
              />
              <Show when={s?.connected} fallback={
                <Show when={s?.authNeeded} fallback={t("tailscale.status.disconnected")}>
                  {t("tailscale.status.authNeeded")}
                </Show>
              }>
                {t("tailscale.status.connected")}
              </Show>
            </Show>
            <Show when={!loading() && !s}>
              {t("tailscale.status.disconnected")}
            </Show>
          </span>
        </div>

        <Show when={s?.connected}>
          <div class="settings-field-row">
            <span class="settings-field-label">{t("tailscale.ips")}</span>
            <div class="settings-ip-list">
              {s!.tailscaleIPs.map((ip) => (
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

          <Show when={s?.hostname}>
            <div class="settings-field-row">
              <span class="settings-field-label">{t("tailscale.hostname")}</span>
              <span class="settings-field-value">{s!.hostname}</span>
            </div>
          </Show>
        </Show>
      </div>

      <Show when={s?.authNeeded}>
        <div class="settings-card">
          <h4 class="settings-card-title">{t("tailscale.auth.authKey.label")}</h4>
          <div class="settings-field-row">
            <input
              type="password"
              class="text-input"
              placeholder={t("tailscale.auth.authKey.placeholder")}
              value={authKey()}
              onInput={(e) => setAuthKey(e.currentTarget.value)}
            />
            <button
              type="button"
              class="selector-button selector-button-primary"
              disabled={authKeySaving() || !authKey().trim()}
              onClick={handleSaveAuthKey}
            >
              <Show when={authKeySaving()} fallback={t("tailscale.auth.authKey.save")}>
                <Loader2 class="settings-icon-spin" />
              </Show>
            </button>
          </div>
          <Show when={authKeyError()}>
            <p class="settings-error">{t("tailscale.auth.authKey.error", { error: authKeyError() })}</p>
          </Show>
        </div>

        <Show when={loginUrl()}>
          <div class="settings-card">
            <h4 class="settings-card-title">{t("tailscale.auth.interactive.label")}</h4>
            <div class="settings-field-row">
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleOpenLoginUrl}
              >
                <ExternalLink class="w-4 h-4" />
                {t("tailscale.auth.interactive.open")}
              </button>
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleCopyLoginUrl}
              >
                <Copy class="w-4 h-4" />
                {t("tailscale.auth.interactive.copy")}
              </button>
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleShowQr}
              >
                <QrCode class="w-4 h-4" />
                {t("tailscale.auth.interactive.qr")}
              </button>
            </div>
            <Show when={qrDataUrl()}>
              <div class="settings-qr-container">
                <img src={qrDataUrl()!} alt="QR Code" class="settings-qr-image" />
              </div>
            </Show>
          </div>
        </Show>
      </Show>

      <div class="settings-card">
        <div class="settings-field-row">
          <Show when={s?.connected}>
            <button
              type="button"
              class="selector-button selector-button-danger"
              onClick={handleStop}
            >
              {t("tailscale.disconnect")}
            </button>
          </Show>
          <button
            type="button"
            class="selector-button selector-button-secondary"
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
