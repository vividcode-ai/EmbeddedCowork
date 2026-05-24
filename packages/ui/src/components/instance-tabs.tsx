import { Component, For, Show } from "solid-js"
import InstanceTab from "./instance-tab"
import KeyboardHint from "./keyboard-hint"
import { Plus, Settings, Terminal } from "lucide-solid"
import { keyboardRegistry } from "../lib/keyboard-registry"
import { useI18n } from "../lib/i18n"
import { openSettings } from "../stores/settings-screen"
import { showCommandPalette } from "../stores/command-palette"
import type { AppTabRecord } from "../stores/app-tabs"

interface InstanceTabsProps {
  tabs: AppTabRecord[]
  activeTabId: string | null
  onSelect: (tabId: string) => void
  onClose: (tabId: string) => void
  onNew: () => void
}

const InstanceTabs: Component<InstanceTabsProps> = (props) => {
  const { t } = useI18n()

  return (
    <div class="tab-bar tab-bar-instance">
      <div class="tab-container" role="tablist">
        <div class="tab-scroll">
          <div class="tab-strip">
            <div class="tab-strip-tabs">
              <For each={props.tabs}>
                {(tab) =>
                  tab.kind === "instance" ? (
                    <InstanceTab
                      instance={tab.instance}
                      active={tab.id === props.activeTabId}
                      onSelect={() => props.onSelect(tab.id)}
                      onClose={() => props.onClose(tab.id)}
                    />
                  ) : (
                    <div class={`tab-pill ${tab.id === props.activeTabId ? "tab-pill-active" : ""}`}>
                      <button class="tab-pill-button" onClick={() => props.onSelect(tab.id)}>
                        <span class="truncate max-w-[180px]">{tab.sidecarTab.name}</span>
                      </button>
                      <button class="tab-pill-close" onClick={() => props.onClose(tab.id)} aria-label={tab.sidecarTab.name}>
                        ×
                      </button>
                    </div>
                  )}
              </For>
              <button
                class="new-tab-button"
                onClick={props.onNew}
                title={t("instanceTabs.new.title")}
                aria-label={t("instanceTabs.new.ariaLabel")}
              >
                <Plus class="w-4 h-4" />
              </button>
            </div>
            <div class="tab-strip-spacer" />
            <Show when={props.tabs.length > 1}>
              <div class="tab-shortcuts">
                <KeyboardHint
                  shortcuts={[keyboardRegistry.get("instance-prev")!, keyboardRegistry.get("instance-next")!].filter(
                    Boolean,
                  )}
                />
              </div>
            </Show>
              <button
                class="new-tab-button"
                onClick={() => {
                  const tab = props.tabs.find((t) => t.id === props.activeTabId)
                  if (tab?.kind === "instance") showCommandPalette(tab.instance.id)
                }}
                title={t("instanceShell.commandPalette.openAriaLabel")}
                aria-label={t("instanceShell.commandPalette.openAriaLabel")}
              >
                <Terminal class="w-4 h-4" />
              </button>
              <button
                class="new-tab-button"
                onClick={() => openSettings("appearance")}
                title={t("settings.open.title")}
                aria-label={t("settings.open.ariaLabel")}
              >
                <Settings class="w-4 h-4" />
              </button>
          </div>
        </div>
      </div>
    </div>

  )
}

export default InstanceTabs
