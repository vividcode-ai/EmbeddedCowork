import { Dialog } from "@kobalte/core/dialog"
import { For, Show, createEffect, createMemo, createSignal } from "solid-js"
import { fetchProviders } from "../stores/sessions"
import { getRootClient } from "../stores/worktrees"
import type { Model } from "../types/session"
import { useI18n } from "../lib/i18n"
import { getLogger } from "../lib/logger"
const log = getLogger("session")

interface AddModelDialogProps {
  open: boolean
  instanceId: string
  onModelChange: (model: { providerId: string; modelId: string }) => Promise<void>
  onClose: () => void
  preloadedProviders?: FlatModel[]
}

interface FlatModel extends Model {
  providerName: string
  key: string
  searchText: string
}

interface ConnectedProviderInfo {
  providerId: string
  providerName: string
}

type ViewState = "list" | "enter-key" | "select-model"

export function mapProvidersToFlatModels(providers: Array<{ id: string; name: string; models: Record<string, any> }>): FlatModel[] {
  const list: FlatModel[] = []
  for (const p of providers) {
    const modelEntries = Array.isArray(p.models) ? p.models.map((m: any) => [m.id, m] as const) : Object.entries(p.models)
    for (const [id, m] of modelEntries) {
      const modelId = id
      list.push({
        id: modelId,
        name: m.name,
        providerId: p.id,
        providerName: p.name,
        key: `${p.id}/${modelId}`,
        searchText: `${m.name} ${p.name} ${p.id} ${modelId} ${p.id}/${modelId}`,
        limit: m.limit,
        cost: m.cost,
        variantKeys: Object.keys(m.variants ?? {}),
      })
    }
  }
  return list
}

export default function AddModelDialog(props: AddModelDialogProps) {
  const { t } = useI18n()
  const [view, setView] = createSignal<ViewState>("list")
  const [unconnectedModels, setUnconnectedModels] = createSignal<FlatModel[]>([])
  const [search, setSearch] = createSignal("")
  const [pendingModel, setPendingModel] = createSignal<FlatModel | null>(null)
  const [apiKey, setApiKey] = createSignal("")
  const [pollError, setPollError] = createSignal<string | null>(null)
  const [connecting, setConnecting] = createSignal(false)
  const [successMsg, setSuccessMsg] = createSignal("")
  const [connectedProvider, setConnectedProvider] = createSignal<ConnectedProviderInfo | null>(null)
  const [selectedModelId, setSelectedModelId] = createSignal("")
  const [selectedVariant, setSelectedVariant] = createSignal("")
  let connectBtnRef!: HTMLButtonElement

  const visibleProviders = createMemo(() => {
    const query = search().toLowerCase().trim()
    const all = unconnectedModels()
    const seen = new Set<string>()
    const result: Array<{ providerId: string; providerName: string }> = []
    const source = !query
      ? all
      : all.filter((m) =>
          m.providerName.toLowerCase().includes(query) || m.providerId.toLowerCase().includes(query)
        )
    for (const m of source) {
      if (!seen.has(m.providerId)) {
        seen.add(m.providerId)
        result.push({ providerId: m.providerId, providerName: m.providerName })
      }
    }
    return result
  })

  const handleOpen = async () => {
    setView("list")
    setSearch("")
    setPendingModel(null)
    setApiKey("")
    setPollError(null)
    setConnectedProvider(null)
    setSelectedModelId("")
    setSelectedVariant("")
    if (props.preloadedProviders?.length) {
      setUnconnectedModels(props.preloadedProviders)
      return
    }
    try {
      const rootClient = getRootClient(props.instanceId)
      const cfg = await rootClient.config.providers()
      if (cfg.data?.providers?.length) {
        setUnconnectedModels(mapProvidersToFlatModels(cfg.data.providers))
      }
    } catch (error) {
      log.error("Failed to fetch providers", error)
    }
  }

  const handleModelClick = (model: FlatModel) => {
    setPendingModel(model)
    setApiKey("")
    setPollError(null)
    setView("enter-key")
  }

  const handleProviderClick = (providerId: string) => {
    const model = unconnectedModels().find((m) => m.providerId === providerId)
    if (model) handleModelClick(model)
  }

  const handleApiKeySubmit = async (event: MouseEvent) => {
    event.preventDefault()
    event.stopPropagation()
    const model = pendingModel()
    if (!model || !apiKey().trim()) return

    connectBtnRef.textContent = "连接中..."
    connectBtnRef.disabled = true

    try {
      const rootClient = getRootClient(props.instanceId)
      await rootClient.global.config.update({
        config: {
          provider: {
            [model.providerId]: {
              options: { apiKey: apiKey().trim() },
            },
          },
        },
      })

      const startTime = Date.now()
      const maxDuration = 10000
      const poll = async () => {
        if (Date.now() - startTime > maxDuration) {
          connectBtnRef.textContent = t("modelSelector.apiKey.submit")
          connectBtnRef.disabled = false
          setPollError(t("modelSelector.apiKey.failed"))
          return
        }
        try {
          const res = await rootClient.provider.list()
          if (res.data?.connected?.includes(model.providerId)) {
            setSuccessMsg(t("modelSelector.apiKey.connected"))
            await fetchProviders(props.instanceId)
            await new Promise((r) => setTimeout(r, 800))
            setSuccessMsg("")
            setConnectedProvider({ providerId: model.providerId, providerName: model.providerName })
            setSelectedModelId("")
            setSelectedVariant("")
            setView("select-model")
            return
          }
        } catch {
          // ignore polling errors
        }
        setTimeout(poll, 200)
      }
      poll()
    } catch {
      connectBtnRef.textContent = t("modelSelector.apiKey.submit")
      connectBtnRef.disabled = false
      setPollError(t("modelSelector.apiKey.failed"))
    }
  }

  const providerModels = createMemo(() => {
    const cp = connectedProvider()
    if (!cp) return []
    return unconnectedModels().filter((m) => m.providerId === cp.providerId)
  })

  const handleConfirmModel = async (modelId: string) => {
    const cp = connectedProvider()
    if (!cp || !modelId) return
    try {
      await props.onModelChange({ providerId: cp.providerId, modelId })
    } catch (error) {
      log.error("Failed to confirm model selection", error)
    }
    props.onClose()
  }

  createEffect(() => {
    if (props.open) {
      if (props.preloadedProviders?.length && unconnectedModels().length === 0) {
        setUnconnectedModels(props.preloadedProviders)
        return
      }
      handleOpen()
    }
  })

  return (
    <Dialog open={props.open} onOpenChange={(open) => !open && props.onClose()}>
      <Dialog.Overlay class="modal-overlay" />
      <Show when={props.open}>
        <div style={{
          position: "fixed",
          inset: "0",
          "z-index": "50",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          padding: "16px",
          "pointer-events": "none",
        }}>
          <Dialog.Content class="modal-surface selector-popover" style={{ width: "530px", "max-width": "min(100%, calc(100vw - 32px))", height: "330px", "max-height": "min(330px, calc(100dvh - 32px))", display: "flex", "flex-direction": "column", "pointer-events": "auto" }}>
          <Dialog.Title class="sr-only">{t("modelSelector.addModel")}</Dialog.Title>

          <Show when={view() === "list"}>
            <div class="selector-search-container">
              <input
                class="selector-search-input"
                placeholder={t("modelSelector.addModel.search")}
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
              />
            </div>
            <div class="selector-listbox" style={{ "flex": "1", "overflow-y": "auto" }}>
              <Show when={unconnectedModels().length > 0}>
                <Show when={visibleProviders().length > 0}>
                  <For each={visibleProviders()}>
                    {(p) => (
                      <div
                        class="selector-option"
                        onClick={(event) => {
                          event.preventDefault()
                          event.stopPropagation()
                          handleProviderClick(p.providerId)
                        }}
                      >
                        <div class="selector-option-content">
                          <span class="selector-option-label">{p.providerName}</span>
                          <span class="selector-option-description">{p.providerId}</span>
                        </div>
                      </div>
                    )}
                  </For>
                </Show>
                <Show when={visibleProviders().length === 0}>
                  <div class="selector-empty-state">{t("modelSelector.addModel.empty")}</div>
                </Show>
              </Show>
              <Show when={unconnectedModels().length === 0}>
                <div class="selector-empty-state">{t("modelSelector.apiKey.connecting")}</div>
              </Show>
            </div>
            <div class="selector-footer" style={{ display: "flex", "justify-content": "flex-end", padding: "8px" }}>
              <button
                type="button"
                class="selector-button selector-button-secondary"
                onClick={props.onClose}
              >
                {t("modelSelector.apiKey.cancel")}
              </button>
            </div>
          </Show>

          <Show when={view() === "enter-key"}>
            <div class="selector-search-container">
              <button
                type="button"
                class="selector-back-button"
                onClick={() => setView("list")}
              >
                <span style="font-size:14px">&lt;</span> <span style="font-size:13px">{t("modelSelector.addModel.back")}</span>
              </button>
            </div>
            <div class="selector-api-key-area">
              <p class="selector-api-key-label">
                {t("modelSelector.apiKey.required", { provider: pendingModel()!.providerName })}
              </p>
              <input
                type="password"
                class="selector-api-key-input"
                placeholder={t("modelSelector.apiKey.placeholder")}
                value={apiKey()}
                onInput={(e) => setApiKey(e.currentTarget.value)}
              />
              <div class="selector-api-key-actions">
                <button
                  ref={connectBtnRef}
                  type="button"
                  class="selector-button selector-button-primary"
                  onClick={handleApiKeySubmit}
                  disabled={!apiKey().trim()}
                >
                  {t("modelSelector.apiKey.submit")}
                </button>
                <button
                  type="button"
                  class="selector-button selector-button-secondary"
                  onClick={() => setView("list")}
                  disabled={connecting()}
                >
                  {t("modelSelector.apiKey.cancel")}
                </button>
              </div>
              <Show when={pollError()}>
                <p class="selector-api-key-error" style="margin:8px 0 0">{pollError()}</p>
              </Show>
              <Show when={successMsg()}>
                <p style="font-size:14px;font-weight:600;color:var(--status-success);margin:8px 0 0">{successMsg()}</p>
              </Show>
            </div>
          </Show>

          <Show when={view() === "select-model" && connectedProvider()}>
            <div class="selector-api-key-area" style={{ flex: "1", "overflow-y": "auto" }}>
              <p class="selector-api-key-label">{t("modelSelector.selectModel.title", { provider: connectedProvider()!.providerName })}</p>
              <div style={{ "margin-top": "12px" }}>
                <For each={providerModels()}>
                  {(model) => {
                    const isSelected = () => selectedModelId() === model.id
                    const hasVariants = () => model.variantKeys && model.variantKeys.length > 0
                    return (
                      <div style={{ "margin-bottom": "8px" }}>
                        <div
                          class="selector-option"
                          data-selected={isSelected()}
                          onClick={() => {
                            if (hasVariants()) {
                              if (isSelected()) {
                                setSelectedModelId("")
                                setSelectedVariant("")
                              } else {
                                setSelectedModelId(model.id)
                                setSelectedVariant(model.variantKeys![0])
                              }
                            } else {
                              handleConfirmModel(model.id)
                            }
                          }}
                          style={isSelected() ? {
                            "background": "var(--accent-bg)",
                            "border": "1px solid var(--accent)",
                            "border-radius": "6px",
                          } : undefined}
                        >
                          <div class="selector-option-content">
                            <span class="selector-option-label">{model.name}</span>
                            <span class="selector-option-description">
                              {model.providerName} • {model.providerId}/{model.id}
                            </span>
                          </div>
                        </div>
                        <Show when={isSelected() && hasVariants()}>
                          <div style={{ display: "flex", gap: "6px", "flex-wrap": "wrap", padding: "6px 12px 2px" }}>
                            <For each={model.variantKeys}>
                              {(vk) => (
                                <button
                                  type="button"
                                  class="selector-button"
                                  classList={{
                                    "selector-button-primary": selectedVariant() === vk,
                                    "selector-button-secondary": selectedVariant() !== vk,
                                  }}
                                  style={{ "font-size": "12px", padding: "2px 10px" }}
                                  onClick={() => handleConfirmModel(model.id)}
                                >
                                  {vk}
                                </button>
                              )}
                            </For>
                          </div>
                        </Show>
                      </div>
                    )
                  }}
                </For>
              </div>
              <Show when={providerModels().length === 0}>
                <p style={{ "font-size": "13px", color: "var(--text-tertiary)", "margin-top": "12px" }}>
                  {t("modelSelector.selectModel.noModels")}
                </p>
              </Show>
            </div>
          </Show>
          </Dialog.Content>
        </div>
      </Show>
    </Dialog>
  )
}
