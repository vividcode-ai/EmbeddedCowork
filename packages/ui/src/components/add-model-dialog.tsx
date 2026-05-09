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

type ViewState = "list" | "enter-key"

export function mapProvidersToFlatModels(providers: Array<{ id: string; name: string; models: Record<string, any> }>): FlatModel[] {
  const list: FlatModel[] = []
  for (const p of providers) {
    const modelEntries = Array.isArray(p.models) ? p.models.map((m: any) => [m.id, m] as const) : Object.entries(p.models)
    for (const [id, m] of modelEntries) {
      const modelId = m.id ?? id
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
  let connectBtnRef!: HTMLButtonElement

  const filteredUnconnected = createMemo(() => {
    const query = search().toLowerCase().trim()
    const all = unconnectedModels()
    if (!query) return all
    const matchingIds = new Set(
      all
        .filter((m) =>
          m.providerName.toLowerCase().includes(query) ||
          m.providerId.toLowerCase().includes(query)
        )
        .map((m) => m.providerId)
    )
    if (matchingIds.size === 0) return []
    return all.filter((m) => matchingIds.has(m.providerId))
  })

  const groupedModels = createMemo(() => {
    const map = new Map<string, FlatModel[]>()
    for (const m of filteredUnconnected()) {
      const list = map.get(m.providerId) ?? []
      list.push(m)
      map.set(m.providerId, list)
    }
    return [...map.entries()]
  })

  const handleOpen = async () => {
    setView("list")
    setSearch("")
    setPendingModel(null)
    setApiKey("")
    setPollError(null)
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
            setSuccessMsg("连接成功")
            await fetchProviders(props.instanceId)
            await new Promise((r) => setTimeout(r, 1000))
            setSuccessMsg("")
            props.onClose()
            await props.onModelChange({ providerId: model.providerId, modelId: model.id })
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
                <Show when={groupedModels().length > 0}>
                  <For each={groupedModels()}>
                    {([providerId, models]) => (
                      <>
                        <div class="selector-section-title" style={{ "padding": "8px 12px 4px", "font-size": "11px", "text-transform": "uppercase", "letter-spacing": "0.05em" }}>{providerId}</div>
                        <For each={models}>
                          {(model) => (
                            <div
                              class="selector-option"
                              onClick={(event) => {
                                event.preventDefault()
                                event.stopPropagation()
                                handleModelClick(model)
                              }}
                            >
                              <div class="selector-option-content">
                                <span class="selector-option-label">{model.name}</span>
                                <span class="selector-option-description">
                                  {model.providerName} • {model.providerId}/{model.id}
                                </span>
                              </div>
                            </div>
                          )}
                        </For>
                      </>
                    )}
                  </For>
                </Show>
                <Show when={groupedModels().length === 0}>
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
          </Dialog.Content>
        </div>
      </Show>
    </Dialog>
  )
}
