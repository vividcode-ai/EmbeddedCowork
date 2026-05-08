import { Combobox } from "@kobalte/core/combobox"
import { For, Show, createEffect, createMemo, createSignal } from "solid-js"
import { providers, fetchProviders } from "../stores/sessions"
import { getRootClient } from "../stores/worktrees"
import { ChevronDown, Star } from "lucide-solid"
import type { Model } from "../types/session"
import { useI18n } from "../lib/i18n"
import { getLogger } from "../lib/logger"
import { uiState, toggleFavoriteModelPreference } from "../stores/preferences"
const log = getLogger("session")


interface ModelSelectorProps {
  instanceId: string
  sessionId: string
  currentModel: { providerId: string; modelId: string }
  onModelChange: (model: { providerId: string; modelId: string }) => Promise<void>
}

interface FlatModel extends Model {
  providerName: string
  key: string
  searchText: string
}

type ViewState = "browse" | "add-model" | "enter-key" | "connecting" | "error"

export default function ModelSelector(props: ModelSelectorProps) {
  const { t } = useI18n()
  const instanceProviders = () => providers().get(props.instanceId) || []
  const [isOpen, setIsOpen] = createSignal(false)
  const [manualAll, setManualAll] = createSignal(false)
  const [explicitFavorites, setExplicitFavorites] = createSignal(false)
  const [autoFavoritesEligibleAtOpen, setAutoFavoritesEligibleAtOpen] = createSignal(false)
  const [searchDirty, setSearchDirty] = createSignal(false)
  const [initialQuery, setInitialQuery] = createSignal("")
  const [initialQueryReady, setInitialQueryReady] = createSignal(false)
  const [inputValue, setInputValue] = createSignal("")
  const [view, setView] = createSignal<ViewState>("browse")
  const [addSearch, setAddSearch] = createSignal("")
  const [unconnectedModels, setUnconnectedModels] = createSignal<FlatModel[]>([])
  const [pendingModel, setPendingModel] = createSignal<FlatModel | null>(null)
  const [apiKey, setApiKey] = createSignal("")
  const [pollError, setPollError] = createSignal<string | null>(null)
  let triggerRef!: HTMLButtonElement
  let searchInputRef!: HTMLInputElement
  let listboxRef!: HTMLUListElement
  let suppressNextClose = false
  let wasFavoritesOnlyEnabled = false
  let wasCurrentModelFavorite = false

  createEffect(() => {
    if (instanceProviders().length === 0) {
      fetchProviders(props.instanceId).catch((error) => log.error("Failed to fetch providers", error))
    }
  })

  const allModels = createMemo<FlatModel[]>(() =>
    instanceProviders().flatMap((p) =>
      p.models.map((m) => ({
        ...m,
        providerName: p.name,
        key: `${m.providerId}/${m.id}`,
        searchText: `${m.name} ${p.name} ${m.providerId} ${m.id} ${m.providerId}/${m.id}`,
      })),
    ),
  )

  const favoriteKeySet = createMemo(() => {
    const result = new Set<string>()
    for (const item of uiState().models.favorites ?? []) {
      if (item.providerId && item.modelId) {
        result.add(`${item.providerId}/${item.modelId}`)
      }
    }
    return result
  })

  const favoriteModels = createMemo<FlatModel[]>(() => {
    const keys = favoriteKeySet()
    if (keys.size === 0) return []
    return allModels().filter((m) => keys.has(m.key))
  })

  const hasFavorites = createMemo(() => favoriteModels().length > 0)

  const currentModelValue = createMemo(() =>
    allModels().find((m) => m.providerId === props.currentModel.providerId && m.id === props.currentModel.modelId),
  )

  const currentModelIsFavorite = createMemo(() => {
    const current = props.currentModel
    return favoriteKeySet().has(`${current.providerId}/${current.modelId}`)
  })

  const currentModelKey = createMemo(() => {
    const current = props.currentModel
    return `${current.providerId}/${current.modelId}`
  })

  const searchActive = createMemo(() => {
    if (!searchDirty()) return false
    const next = inputValue().trim()
    return next.length > 0
  })

  const favoritesOnlyEnabled = createMemo(() => {
    if (searchActive()) return false
    if (manualAll()) return false
    if (!hasFavorites()) return false
    return explicitFavorites() || autoFavoritesEligibleAtOpen()
  })

  const visibleOptions = createMemo<FlatModel[]>(() => {
    if (!favoritesOnlyEnabled()) {
      return allModels()
    }
    return favoriteModels()
  })

  const filteredUnconnected = createMemo(() => {
    const query = addSearch().toLowerCase().trim()
    const all = unconnectedModels()
    if (!query) return all
    return all.filter((m) =>
      m.providerName.toLowerCase().includes(query) ||
      m.providerId.toLowerCase().includes(query)
    )
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

  const handleChange = async (value: FlatModel | null) => {
    if (!value) return
    await props.onModelChange({ providerId: value.providerId, modelId: value.id })
  }

  const handleAddModel = async () => {
    try {
      const rootClient = getRootClient(props.instanceId)
      const res = await rootClient.provider.list()
      const connectedSet = new Set(res.data?.connected ?? [])
      const list: FlatModel[] = []
      for (const p of res.data?.all ?? []) {
        if (connectedSet.has(p.id)) continue
        for (const [id, m] of Object.entries(p.models)) {
          list.push({
            id,
            name: m.name,
            providerId: p.id,
            providerName: p.name,
            key: `${p.id}/${id}`,
            searchText: `${m.name} ${p.name} ${p.id} ${id} ${p.id}/${id}`,
            limit: m.limit,
            cost: m.cost,
            variantKeys: Object.keys(m.variants ?? {}),
          })
        }
      }
      setUnconnectedModels(list)
      setAddSearch("")
      setView("add-model")
    } catch (error) {
      log.error("Failed to fetch unconnected providers", error)
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

    setView("connecting")
    setPollError(null)

    try {
      const rootClient = getRootClient(props.instanceId)
      await rootClient.config.update({
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
          setPollError(t("modelSelector.apiKey.failed"))
          setView("error")
          return
        }
        try {
          const res = await rootClient.provider.list()
          if (res.data?.connected?.includes(model.providerId)) {
            await fetchProviders(props.instanceId)
            suppressNextClose = true
            setTimeout(() => { suppressNextClose = false }, 0)
            setIsOpen(false)
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
      setPollError(t("modelSelector.apiKey.failed"))
      setView("error")
    }
  }

  const handleCancel = () => {
    setView("browse")
    setIsOpen(false)
  }

  const customFilter = (option: FlatModel, rawInput: string) => {
    if (!searchDirty()) return true
    return option.searchText.toLowerCase().includes(rawInput.toLowerCase())
  }

  createEffect(() => {
    if (isOpen()) {
      setView("browse")
      setAddSearch("")
      setManualAll(false)
      setExplicitFavorites(false)
      setAutoFavoritesEligibleAtOpen(hasFavorites() && currentModelIsFavorite())
      setSearchDirty(false)
      setInitialQuery("")
      setInputValue("")
      setInitialQueryReady(false)
      setTimeout(() => {
        const seeded = searchInputRef?.value ?? ""
        setInitialQuery(seeded)
        setInputValue(seeded)
        setInitialQueryReady(true)
        searchInputRef?.focus()
        searchInputRef?.select()
      }, 100)
    } else {
      setInitialQueryReady(false)
      setSearchDirty(false)
      setAutoFavoritesEligibleAtOpen(false)
    }
  })

  createEffect(() => {
    if (!isOpen()) {
      wasFavoritesOnlyEnabled = favoritesOnlyEnabled()
      wasCurrentModelFavorite = currentModelIsFavorite()
      return
    }

    const nowFavoritesOnlyEnabled = favoritesOnlyEnabled()
    const nowCurrentModelFavorite = currentModelIsFavorite()

    if (wasFavoritesOnlyEnabled && !nowFavoritesOnlyEnabled && wasCurrentModelFavorite && !nowCurrentModelFavorite) {
      setTimeout(() => {
        const key = currentModelKey()
        const target = listboxRef?.querySelector(`[data-key="${key}"]`) as HTMLElement | null
        target?.scrollIntoView({ block: "nearest" })
      }, 0)
    }

    wasFavoritesOnlyEnabled = nowFavoritesOnlyEnabled
    wasCurrentModelFavorite = nowCurrentModelFavorite
  })

  const handleSearchInput = (event: InputEvent & { currentTarget: HTMLInputElement }) => {
    const next = event.currentTarget.value
    setInputValue(next)
    if (!initialQueryReady()) return
    if (searchDirty()) return
    if (next !== initialQuery()) {
      setSearchDirty(true)
    }
  }

  const preventListboxPress = (event: PointerEvent | MouseEvent) => {
    event.preventDefault()
    event.stopImmediatePropagation?.()
    event.stopPropagation()
    suppressNextClose = true
    setTimeout(() => {
      suppressNextClose = false
    }, 0)
  }

  const toggleFavoritesOnly = () => {
    if (!hasFavorites()) return
    if (searchActive()) return

    if (favoritesOnlyEnabled()) {
      setManualAll(true)
      setExplicitFavorites(false)
      setAutoFavoritesEligibleAtOpen(false)
      return
    }

    setExplicitFavorites(true)
    setManualAll(false)
  }

  const showAllModels = () => {
    setManualAll(true)
    setExplicitFavorites(false)
    setAutoFavoritesEligibleAtOpen(false)
    setTimeout(() => searchInputRef?.focus(), 0)
  }

  return (
    <div class="sidebar-selector">
      <Combobox<FlatModel>
        open={isOpen()}
        value={currentModelValue()}
        onChange={handleChange}
        onOpenChange={(next) => {
          if (!next && suppressNextClose) return
          setIsOpen(next)
        }}
        options={visibleOptions()}
        optionValue="key"
        optionTextValue="searchText"
        optionLabel="name"
        placeholder={t("modelSelector.placeholder.search")}
        defaultFilter={customFilter}
        allowsEmptyCollection
        itemComponent={(itemProps) => {
          const isFavorite = () => favoriteKeySet().has(itemProps.item.rawValue.key)
          return (
            <Combobox.Item
              item={itemProps.item}
              class="selector-option"
            >
              <>
                <div class="selector-option-content">
                  <Combobox.ItemLabel class="selector-option-label">{itemProps.item.rawValue.name}</Combobox.ItemLabel>
                  <Combobox.ItemDescription class="selector-option-description">
                    {itemProps.item.rawValue.providerName} • {itemProps.item.rawValue.providerId}/{itemProps.item.rawValue.id}
                  </Combobox.ItemDescription>
                </div>
                <Combobox.ItemIndicator class="selector-option-indicator">
                  <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                  </svg>
                </Combobox.ItemIndicator>
                <button
                  type="button"
                  class="selector-option-star"
                  data-active={isFavorite()}
                  aria-label={
                    isFavorite()
                      ? t("modelSelector.favorite.remove")
                      : t("modelSelector.favorite.add")
                  }
                  onPointerDown={preventListboxPress}
                  onPointerUp={preventListboxPress}
                  onMouseDown={preventListboxPress}
                  onMouseUp={preventListboxPress}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter" && event.key !== " ") return
                    event.preventDefault()
                    event.stopPropagation()
                    suppressNextClose = true
                    setTimeout(() => {
                      suppressNextClose = false
                    }, 0)
                  }}
                  onClick={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    toggleFavoriteModelPreference({
                      providerId: itemProps.item.rawValue.providerId,
                      modelId: itemProps.item.rawValue.id,
                    })
                  }}
                >
                  <Star
                    class="w-4 h-4"
                    fill={isFavorite() ? "currentColor" : "none"}
                  />
                </button>
              </>
            </Combobox.Item>
          )
        }}
      >
        <Combobox.Control class="relative w-full" data-model-selector-control>
          <Combobox.Input class="sr-only" data-model-selector />
          <Combobox.Trigger
            ref={triggerRef}
            class="selector-trigger"
          >
            <div class="selector-trigger-label selector-trigger-label--stacked flex-1 min-w-0">
              <span class="selector-trigger-primary selector-trigger-primary--align-left">
                {t("modelSelector.trigger.primary", { model: currentModelValue()?.name ?? t("modelSelector.none") })}
              </span>
          {currentModelValue() && (
                <span class="selector-trigger-secondary" dir="ltr">
                  {currentModelValue()!.providerId}/{currentModelValue()!.id}
                </span>
              )}
            </div>
            <Combobox.Icon class="selector-trigger-icon">
              <ChevronDown class="w-3 h-3" />
            </Combobox.Icon>
          </Combobox.Trigger>
        </Combobox.Control>

        <Combobox.Portal>
          <Combobox.Content class="selector-popover">
            <div class="selector-search-container" style={{ display: view() === "browse" ? undefined : "none" }}>
              <div class="selector-input-group">
                <Combobox.Input
                  ref={searchInputRef}
                  class="selector-search-input flex-1 min-w-0"
                  placeholder={t("modelSelector.placeholder.search")}
                  onInput={handleSearchInput}
                />
                <button
                  type="button"
                  class="selector-favorites-toggle"
                  aria-label={t("modelSelector.favoritesOnly.toggle.ariaLabel")}
                  aria-pressed={favoritesOnlyEnabled()}
                  disabled={!hasFavorites() || searchActive()}
                  data-active={favoritesOnlyEnabled()}
                  onClick={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    toggleFavoritesOnly()
                  }}
                >
                  <Star class="w-4 h-4" fill={favoritesOnlyEnabled() ? "currentColor" : "none"} />
                </button>
              </div>
            </div>
            <Combobox.Listbox ref={listboxRef} class="selector-listbox" style={{ display: view() === "browse" ? undefined : "none" }} />
            <div class="selector-footer" style={{ display: view() === "browse" ? undefined : "none" }}>
              <button
                type="button"
                class="selector-option selector-option-action w-full"
                onClick={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  handleAddModel()
                }}
              >
                <span class="selector-option-label">{t("modelSelector.addModel")}</span>
              </button>
              <button
                type="button"
                class="selector-option selector-option-action w-full"
                style={{ display: favoritesOnlyEnabled() && !searchActive() ? "flex" : "none" }}
                onMouseDown={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                }}
                onPointerDown={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                }}
                onClick={(event) => {
                  event.preventDefault()
                  event.stopPropagation()
                  showAllModels()
                }}
              >
                <span class="selector-option-label">{t("modelSelector.favoritesOnly.showAll")}</span>
              </button>
            </div>

            <div style={{ display: view() === "add-model" ? undefined : "none" }}>
              <div class="selector-search-container">
                <button
                  type="button"
                  class="selector-back-button"
                  onClick={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    setView("browse")
                  }}
                >
                  ← {t("modelSelector.addModel.back")}
                </button>
              </div>
              <Show when={groupedModels().length > 0}>
                <div class="selector-search-container">
                  <input
                    class="selector-search-input"
                    placeholder={t("modelSelector.addModel.search")}
                    value={addSearch()}
                    onInput={(e) => setAddSearch(e.currentTarget.value)}
                  />
                </div>
                <div class="selector-listbox">
                  <For each={groupedModels()}>
                    {([providerId, models]) => (
                      <>
                        <div class="selector-section-title">{providerId}</div>
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
                </div>
              </Show>
              <Show when={groupedModels().length === 0}>
                <div class="selector-empty-state">{t("modelSelector.addModel.empty")}</div>
              </Show>
            </div>

            <div style={{ display: view() === "enter-key" ? undefined : "none" }}>
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
                    onClick={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      setView("add-model")
                    }}
                  >
                    {t("modelSelector.apiKey.cancel")}
                  </button>
                </div>
              </div>
            </div>

            <div style={{ display: view() === "connecting" ? undefined : "none" }}>
              <div class="selector-api-key-area">
                <p class="selector-api-key-status">
                  {t("modelSelector.apiKey.connecting")}
                </p>
              </div>
            </div>

            <div style={{ display: view() === "error" ? undefined : "none" }}>
              <div class="selector-api-key-area">
                <p class="selector-api-key-error">{pollError()}</p>
                <div class="selector-api-key-actions">
                  <button
                    type="button"
                    class="selector-button selector-button-primary"
                    onClick={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      setView("enter-key")
                    }}
                  >
                    {t("modelSelector.apiKey.retry")}
                  </button>
                  <button
                    type="button"
                    class="selector-button selector-button-secondary"
                    onClick={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      handleCancel()
                    }}
                  >
                    {t("modelSelector.apiKey.cancel")}
                  </button>
                </div>
              </div>
            </div>
          </Combobox.Content>
        </Combobox.Portal>
      </Combobox>
    </div>
  )
}
