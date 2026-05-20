import { Select } from "@kobalte/core/select"
import { createEffect, createMemo, createSignal, Show, type Component } from "solid-js"
import { ChevronDown, Database, RefreshCw, Terminal } from "lucide-solid"
import OpenCodeBinarySelector from "../opencode-binary-selector"
import EnvironmentVariablesEditor from "../environment-variables-editor"
import { useConfig } from "../../stores/preferences"
import type { ServerLogLevel } from "../../stores/preferences"
import { useI18n } from "../../lib/i18n"
import { isDesktopHost, isElectronHost, isTauriHost, isUpdaterEnabled } from "../../lib/runtime-env"
import { showToastNotification } from "../../lib/notifications"

type LogLevelOption = {
  value: ServerLogLevel
  label: string
}

type SessionStorageOption = {
  value: "project" | "global"
  label: string
}

export const OpenCodeSettingsSection: Component = () => {
  const { t } = useI18n()
  const { serverSettings, updateLastUsedBinary, updateLogLevel, updateSessionStorageMode } = useConfig()
  const [selectedBinary, setSelectedBinary] = createSignal(serverSettings().opencodeBinary || "opencode")
  const logLevelOptions = createMemo<LogLevelOption[]>(() => [
    { value: "DEBUG", label: t("settings.opencode.logLevel.option.debug") },
    { value: "INFO", label: t("settings.opencode.logLevel.option.info") },
    { value: "WARN", label: t("settings.opencode.logLevel.option.warn") },
    { value: "ERROR", label: t("settings.opencode.logLevel.option.error") },
  ])
  const selectedLogLevel = createMemo(
    () => logLevelOptions().find((option) => option.value === serverSettings().logLevel) ?? logLevelOptions()[0],
  )

  const sessionStorageOptions = createMemo<SessionStorageOption[]>(() => [
    { value: "project", label: t("settings.opencode.sessionStorage.option.project") },
    { value: "global", label: t("settings.opencode.sessionStorage.option.global") },
  ])
  const selectedSessionStorage = createMemo(
    () => sessionStorageOptions().find((option) => option.value === serverSettings().sessionStorageMode) ?? sessionStorageOptions()[0],
  )

  createEffect(() => {
    const binary = serverSettings().opencodeBinary || "opencode"
    setSelectedBinary((current) => (current === binary ? current : binary))
  })

  const handleBinaryChange = (binary: string) => {
    setSelectedBinary(binary)
    updateLastUsedBinary(binary)
  }

  return (
    <div class="settings-section-stack">
      <div class="settings-card">
        <div class="settings-card-header">
          <div class="settings-card-heading-with-icon">
            <Terminal class="settings-card-heading-icon" />
            <div>
              <h3 class="settings-card-title">{t("settings.opencode.runtime.title")}</h3>
              <p class="settings-card-subtitle">{t("settings.opencode.runtime.subtitle")}</p>
            </div>
          </div>
          <span class="settings-scope-badge settings-scope-badge-server">{t("settings.scope.server")}</span>
        </div>

        <OpenCodeBinarySelector selectedBinary={selectedBinary()} onBinaryChange={handleBinaryChange} isVisible />
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <div>
            <h3 class="settings-card-title">{t("settings.opencode.logLevel.title")}</h3>
            <p class="settings-card-subtitle">{t("settings.opencode.logLevel.subtitle")}</p>
          </div>
          <span class="settings-scope-badge settings-scope-badge-server">{t("settings.scope.server")}</span>
        </div>
        <div class="settings-card-body">
          <div class="settings-toggle-row settings-toggle-row-compact">
            <div>
              <div class="settings-toggle-title">{t("settings.opencode.logLevel.selector.title")}</div>
              <div class="settings-toggle-caption">{t("settings.opencode.logLevel.selector.subtitle")}</div>
            </div>
            <Select<LogLevelOption>
              value={selectedLogLevel()}
              onChange={(option) => {
                if (!option) return
                updateLogLevel(option.value)
              }}
              options={logLevelOptions()}
              optionValue="value"
              optionTextValue="label"
              itemComponent={(itemProps) => (
                <Select.Item item={itemProps.item} class="selector-option">
                  <Select.ItemLabel class="selector-option-label">{itemProps.item.rawValue.label}</Select.ItemLabel>
                </Select.Item>
              )}
            >
              <Select.Trigger class="selector-trigger" aria-label={t("settings.opencode.logLevel.title")}>
                <div class="flex-1 min-w-0">
                  <Select.Value<LogLevelOption>>
                    {(state) => (
                      <span class="selector-trigger-primary selector-trigger-primary--align-left">
                        {state.selectedOption()?.label}
                      </span>
                    )}
                  </Select.Value>
                </div>
                <Select.Icon class="selector-trigger-icon">
                  <ChevronDown class="w-3 h-3" />
                </Select.Icon>
              </Select.Trigger>

              <Select.Portal>
                <Select.Content class="selector-popover">
                  <Select.Listbox class="selector-listbox" />
                </Select.Content>
              </Select.Portal>
            </Select>
          </div>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <div class="settings-card-heading-with-icon">
            <Database class="settings-card-heading-icon" />
            <div>
              <h3 class="settings-card-title">{t("settings.opencode.sessionStorage.title")}</h3>
              <p class="settings-card-subtitle">{t("settings.opencode.sessionStorage.subtitle")}</p>
            </div>
          </div>
          <span class="settings-scope-badge settings-scope-badge-server">{t("settings.scope.server")}</span>
        </div>
        <div class="settings-card-body">
          <div class="settings-toggle-row settings-toggle-row-compact">
            <div>
              <div class="settings-toggle-title">{t("settings.opencode.sessionStorage.selector.title")}</div>
              <div class="settings-toggle-caption">{t("settings.opencode.sessionStorage.selector.subtitle")}</div>
            </div>
            <Select<SessionStorageOption>
              value={selectedSessionStorage()}
              onChange={(option) => {
                if (!option) return
                updateSessionStorageMode(option.value)
              }}
              options={sessionStorageOptions()}
              optionValue="value"
              optionTextValue="label"
              itemComponent={(itemProps) => (
                <Select.Item item={itemProps.item} class="selector-option">
                  <Select.ItemLabel class="selector-option-label">{itemProps.item.rawValue.label}</Select.ItemLabel>
                </Select.Item>
              )}
            >
              <Select.Trigger class="selector-trigger" aria-label={t("settings.opencode.sessionStorage.title")}>
                <div class="flex-1 min-w-0">
                  <Select.Value<SessionStorageOption>>
                    {(state) => (
                      <span class="selector-trigger-primary selector-trigger-primary--align-left">
                        {state.selectedOption()?.label}
                      </span>
                    )}
                  </Select.Value>
                </div>
                <Select.Icon class="selector-trigger-icon">
                  <ChevronDown class="w-3 h-3" />
                </Select.Icon>
              </Select.Trigger>

              <Select.Portal>
                <Select.Content class="selector-popover">
                  <Select.Listbox class="selector-listbox" />
                </Select.Content>
              </Select.Portal>
            </Select>
          </div>
        </div>
      </div>

      <div class="settings-card">
        <div class="settings-card-header">
          <div>
            <h3 class="settings-card-title">{t("advancedSettings.environmentVariables.title")}</h3>
            <p class="settings-card-subtitle">{t("advancedSettings.environmentVariables.subtitle")}</p>
          </div>
          <span class="settings-scope-badge settings-scope-badge-server">{t("settings.scope.server")}</span>
        </div>
        <EnvironmentVariablesEditor />
      </div>
      <Show when={isDesktopHost() && isUpdaterEnabled()}>
        <div class="settings-card">
          <div class="settings-card-header">
            <div class="settings-card-heading-with-icon">
              <RefreshCw class="settings-card-heading-icon" />
              <div>
                <h3 class="settings-card-title">{t("settings.update.title")}</h3>
                <p class="settings-card-subtitle">{t("settings.update.subtitle")}</p>
              </div>
            </div>
            <span class="settings-scope-badge settings-scope-badge-server">{t("settings.scope.client")}</span>
          </div>
          <div class="settings-card-body">
            <div class="settings-toggle-row settings-toggle-row-compact">
              <div>
                <div class="settings-toggle-title">{t("settings.update.checkForUpdates")}</div>
                <div class="settings-toggle-caption">{t("settings.update.checkForUpdatesDesc")}</div>
              </div>
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={handleCheckForUpdates}
                disabled={checkingUpdate()}
              >
                {checkingUpdate() ? t("settings.update.checking") : t("settings.update.check")}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  )

  const [checkingUpdate, setCheckingUpdate] = createSignal(false)

  async function handleCheckForUpdates() {
    if (!isDesktopHost() || !isUpdaterEnabled()) return
    setCheckingUpdate(true)
    try {
      let result: { updateAvailable: boolean; version?: string }
      if (isElectronHost()) {
        const api = (window as any).electronAPI as any
        result = await (api.checkUpdate?.() ?? Promise.resolve({ updateAvailable: false }))
      } else if (isTauriHost()) {
        const { invoke } = await import("@tauri-apps/api/core")
        const version = await invoke<string | null>("check_update")
        result = { updateAvailable: version != null, version: version ?? undefined }
      } else return

      if (result.updateAvailable) {
        showToastNotification({
          title: t("update.polling.available.title"),
          message: t("update.polling.available.message", { version: result.version ?? "" }),
          variant: "info",
          duration: Number.POSITIVE_INFINITY,
          position: "bottom-right",
          action: {
            label: t("update.polling.install"),
            onClick: async () => {
              if (isElectronHost()) {
                const api = (window as any).electronAPI as any
                await (api.installUpdateV2?.() ?? Promise.resolve())
              } else if (isTauriHost()) {
                const { check } = await import("@tauri-apps/plugin-updater")
                const update = await check()
                if (update) {
                  await update.downloadAndInstall()
                }
                const { invoke } = await import("@tauri-apps/api/core")
                await invoke("restart_app")
              }
            },
          },
        })
      } else {
        showToastNotification({
          title: t("update.alreadyUpToDate"),
          message: "",
          variant: "success",
          duration: 3000,
          position: "bottom-right",
        })
      }
    } catch (err) {
      showToastNotification({
        title: t("update.checkFailed"),
        message: String(err),
        variant: "error",
        duration: 8000,
        position: "bottom-right",
      })
    } finally {
      setCheckingUpdate(false)
    }
  }
}
