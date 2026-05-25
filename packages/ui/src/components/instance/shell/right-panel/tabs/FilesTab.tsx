import { For, Show, Suspense, createSignal, createEffect, lazy, onCleanup, type Accessor, type Component, type JSX } from "solid-js"
import { Dialog } from "@kobalte/core/dialog"
import type { FileNode } from "@opencode-ai/sdk/v2/client"

import { RefreshCw, Save, Minus, Square, Maximize2, X } from "lucide-solid"

const LazyMonacoFileViewer = lazy(() =>
  import("../../../../file-viewer/monaco-file-viewer").then((module) => ({ default: module.MonacoFileViewer })),
)

interface FilesTabProps {
  t: (key: string, vars?: Record<string, any>) => string

  browserPath: Accessor<string>
  browserEntries: Accessor<FileNode[] | null>
  browserLoading: Accessor<boolean>
  browserError: Accessor<string | null>

  browserSelectedPath: Accessor<string | null>
  browserSelectedContent: Accessor<string | null>
  browserSelectedLoading: Accessor<boolean>
  browserSelectedError: Accessor<string | null>
  browserSelectedDirty: Accessor<boolean>
  browserSelectedSaving: Accessor<boolean>

  parentPath: Accessor<string | null>
  scopeKey: Accessor<string>

  onLoadEntries: (path: string) => void
  onRequestOpenFile: (path: string) => void
  onRefresh: () => void
  onSave: (content: string) => void
  onContentChange: (content: string) => void
}

const FilesTab: Component<FilesTabProps> = (props) => {
  const [modalOpen, setModalOpen] = createSignal(false)
  const [windowPos, setWindowPos] = createSignal({ x: 0, y: 0 })
  const [windowSize, setWindowSize] = createSignal<{ w: number; h: number } | null>(null)
  const [isMaximized, setIsMaximized] = createSignal(false)
  let modalContentEl: HTMLDivElement | undefined

  createEffect(() => {
    if (!props.browserSelectedPath()) {
      setModalOpen(false)
    }
  })

  onCleanup(() => {
    document.removeEventListener("mousemove", handleHeaderMouseMove)
    document.removeEventListener("mouseup", handleHeaderMouseUp)
    document.removeEventListener("mousemove", handleResizeMouseMove)
    document.removeEventListener("mouseup", handleResizeMouseUp)
  })

  const handleSave = () => {
    const content = props.browserSelectedContent()
    if (content !== undefined && content !== null) {
      props.onSave(content)
    }
  }

  const handleFileClick = (path: string) => {
    props.onRequestOpenFile(path)
    setModalOpen(true)
    setWindowPos({ x: 0, y: 0 })
    setWindowSize(null)
    setIsMaximized(false)
    applyDefaultStyle()
  }

  const sorted = (): FileNode[] => {
    const entries = props.browserEntries() || []
    return [...entries].sort((a, b) => {
      const aDir = a.type === "directory" ? 0 : 1
      const bDir = b.type === "directory" ? 0 : 1
      if (aDir !== bDir) return aDir - bDir
      return String(a.name || "").localeCompare(String(b.name || ""))
    })
  }

  const headerDisplayedPath = () => props.browserSelectedPath() || props.browserPath()

  const emptyViewerMessage = () => {
    if (props.browserLoading() && props.browserEntries() === null) return props.t("instanceInfo.loading")
    return props.t("instanceShell.filesShell.viewerEmpty")
  }

  const EDGE_THRESHOLD = 8

  // Window drag
  let dragStartX = 0
  let dragStartY = 0
  let dragStartPosX = 0
  let dragStartPosY = 0

  const handleHeaderMouseDown = (e: MouseEvent) => {
    if (isMaximized()) return
    const rect = modalContentEl!.getBoundingClientRect()
    if (e.clientY - rect.top <= EDGE_THRESHOLD) return
    e.preventDefault()
    dragStartX = e.clientX
    dragStartY = e.clientY
    const pos = windowPos()
    dragStartPosX = pos.x
    dragStartPosY = pos.y
    document.addEventListener("mousemove", handleHeaderMouseMove)
    document.addEventListener("mouseup", handleHeaderMouseUp)
  }

  const handleHeaderMouseMove = (e: MouseEvent) => {
    const dx = e.clientX - dragStartX
    const dy = e.clientY - dragStartY
    modalContentEl!.style.transform = `translate(${dragStartPosX + dx}px, ${dragStartPosY + dy}px)`
  }

  const handleHeaderMouseUp = () => {
    document.removeEventListener("mousemove", handleHeaderMouseMove)
    document.removeEventListener("mouseup", handleHeaderMouseUp)
    const el = modalContentEl
    if (!el) return
    const m = el.style.transform.match(/translate\(([-\d.]+)px,\s*([-\d.]+)px\)/)
    if (m) setWindowPos({ x: +m[1], y: +m[2] })
  }

  // Window resize (edge detection)
  let activeResizeEdge: 'top' | 'bottom' | 'left' | 'right' | null = null
  let resizeStartX = 0
  let resizeStartY = 0
  let resizeStartW = 0
  let resizeStartH = 0
  let resizeStartPosX = 0
  let resizeStartPosY = 0

  const handleContentMouseMove = (e: MouseEvent) => {
    if (isMaximized()) return
    const el = modalContentEl!
    const rect = el.getBoundingClientRect()
    const x = e.clientX - rect.left
    const y = e.clientY - rect.top
    const onTop = y <= EDGE_THRESHOLD
    const onBottom = rect.height - y <= EDGE_THRESHOLD
    const onLeft = x <= EDGE_THRESHOLD
    const onRight = rect.width - x <= EDGE_THRESHOLD
    if (!onTop && !onBottom && !onLeft && !onRight) {
      el.style.cursor = ''
      return
    }
    if ((onTop && onLeft) || (onBottom && onRight)) el.style.cursor = 'nwse-resize'
    else if ((onTop && onRight) || (onBottom && onLeft)) el.style.cursor = 'nesw-resize'
    else if (onTop || onBottom) el.style.cursor = 'ns-resize'
    else el.style.cursor = 'ew-resize'
  }

  const handleContentMouseDown = (e: MouseEvent) => {
    if (isMaximized()) return
    const rect = modalContentEl!.getBoundingClientRect()
    const x = e.clientX - rect.left
    const y = e.clientY - rect.top
    const onTop = y <= EDGE_THRESHOLD
    const onBottom = rect.height - y <= EDGE_THRESHOLD
    const onLeft = x <= EDGE_THRESHOLD
    const onRight = rect.width - x <= EDGE_THRESHOLD
    if (!onTop && !onBottom && !onLeft && !onRight) return
    e.preventDefault()
    if (onTop) activeResizeEdge = 'top'
    else if (onBottom) activeResizeEdge = 'bottom'
    else if (onLeft) activeResizeEdge = 'left'
    else if (onRight) activeResizeEdge = 'right'
    resizeStartX = e.clientX
    resizeStartY = e.clientY
    resizeStartW = rect.width
    resizeStartH = rect.height
    const pos = windowPos()
    resizeStartPosX = pos.x
    resizeStartPosY = pos.y
    document.addEventListener("mousemove", handleResizeMouseMove)
    document.addEventListener("mouseup", handleResizeMouseUp)
  }

  const handleResizeMouseMove = (e: MouseEvent) => {
    const edge = activeResizeEdge
    if (!edge) return
    const dx = e.clientX - resizeStartX
    const dy = e.clientY - resizeStartY
    let newW = resizeStartW
    let newH = resizeStartH
    let newX = resizeStartPosX
    let newY = resizeStartPosY
    if (edge === 'right') {
      newW = Math.max(400, resizeStartW + dx)
    } else if (edge === 'left') {
      newW = Math.max(400, resizeStartW - dx)
      newX = resizeStartPosX + (resizeStartW - newW)
    } else if (edge === 'bottom') {
      newH = Math.max(300, resizeStartH + dy)
    } else if (edge === 'top') {
      newH = Math.max(300, resizeStartH - dy)
      newY = resizeStartPosY + (resizeStartH - newH)
    }
    const el = modalContentEl!
    el.style.width = `${newW}px`
    el.style.height = `${newH}px`
    el.style.transform = `translate(${newX}px, ${newY}px)`
  }

  const handleResizeMouseUp = () => {
    activeResizeEdge = null
    document.removeEventListener("mousemove", handleResizeMouseMove)
    document.removeEventListener("mouseup", handleResizeMouseUp)
    const el = modalContentEl
    if (!el) return
    const rect = el.getBoundingClientRect()
    setWindowSize({ w: rect.width, h: rect.height })
    const m = el.style.transform.match(/translate\(([-\d.]+)px,\s*([-\d.]+)px\)/)
    if (m) setWindowPos({ x: +m[1], y: +m[2] })
  }

  // Window controls
  const applyDefaultStyle = () => {
    const el = modalContentEl
    if (!el) return
    el.style.transform = ''
    el.style.width = ''
    el.style.height = ''
  }

  const handleMinimize = () => {
    setWindowPos({ x: 0, y: 0 })
    setWindowSize(null)
    setIsMaximized(false)
    applyDefaultStyle()
  }

  const handleMaximize = () => {
    if (isMaximized()) {
      setWindowPos({ x: 0, y: 0 })
      setWindowSize(null)
      setIsMaximized(false)
      applyDefaultStyle()
    } else {
      setWindowPos({ x: 0, y: 0 })
      setWindowSize(null)
      setIsMaximized(true)
      applyDefaultStyle()
    }
  }

  const handleClose = () => {
    setModalOpen(false)
  }

  const headerClass = () =>
    `file-modal-header${isMaximized() ? " file-modal-header--no-drag" : ""}`

  const contentClass = () =>
    `file-modal-content${isMaximized() ? " file-modal-content--maximized" : ""}`

  const renderModalContent = () => {
    const size = windowSize()
    return (
    <div
      class={contentClass()}
      ref={modalContentEl}
      style={{
        transform: isMaximized() ? undefined : `translate(${windowPos().x}px, ${windowPos().y}px)`,
        width: size ? `${size.w}px` : undefined,
        height: size ? `${size.h}px` : undefined,
      }}
      onMouseMove={handleContentMouseMove}
      onMouseDown={handleContentMouseDown}
    >
      <div class={headerClass()} onMouseDown={handleHeaderMouseDown}>
        <span class="file-modal-path" title={props.browserSelectedPath() || ""}>
          <span class="file-path-text">{props.browserSelectedPath()}</span>
        </span>
        <div class="file-modal-actions">
          <button
            type="button"
            class="file-modal-btn"
            title={props.t("instanceShell.rightPanel.actions.save") || "Save (Ctrl+S)"}
            aria-label={props.t("instanceShell.rightPanel.actions.save") || "Save"}
            disabled={props.browserSelectedSaving() || !props.browserSelectedDirty()}
            onClick={handleSave}
          >
            <Show when={props.browserSelectedSaving()} fallback={<Save class="h-4 w-4" />}>
              <RefreshCw class="h-4 w-4 animate-spin" />
            </Show>
          </button>
          <button
            type="button"
            class="file-modal-btn"
            title={props.t("instanceShell.rightPanel.actions.minimize") || "Minimize"}
            aria-label={props.t("instanceShell.rightPanel.actions.minimize") || "Minimize"}
            onClick={handleMinimize}
          >
            <Minus class="h-4 w-4" />
          </button>
          <button
            type="button"
            class="file-modal-btn"
            title={isMaximized()
              ? (props.t("instanceShell.rightPanel.actions.restore") || "Restore")
              : (props.t("instanceShell.rightPanel.actions.maximize") || "Maximize")}
            aria-label={isMaximized()
              ? (props.t("instanceShell.rightPanel.actions.restore") || "Restore")
              : (props.t("instanceShell.rightPanel.actions.maximize") || "Maximize")}
            onClick={handleMaximize}
          >
            <Show when={isMaximized()} fallback={<Maximize2 class="h-4 w-4" />}>
              <Square class="h-4 w-4" />
            </Show>
          </button>
          <button
            type="button"
            class="file-modal-btn file-modal-btn--close"
            aria-label={props.t("instanceShell.rightPanel.actions.closeModal") || "Close"}
            onClick={handleClose}
          >
            <X class="h-4 w-4" />
          </button>
        </div>
      </div>
      <div class="file-modal-body">
        <div class="file-viewer-panel flex-1">
          <div class="file-viewer-content file-viewer-content--monaco">
            <Show
              when={props.browserSelectedLoading()}
              fallback={
                <Show
                  when={props.browserSelectedError()}
                  fallback={
                    <Show
                      when={
                        props.browserSelectedPath() && props.browserSelectedContent() !== null
                          ? { path: props.browserSelectedPath() as string, content: props.browserSelectedContent() as string }
                          : null
                      }
                      fallback={
                        <div class="file-viewer-empty">
                          <span class="file-viewer-empty-text">{props.t("instanceInfo.loading")}</span>
                        </div>
                      }
                    >
                      {(payload) => (
                        <Suspense
                          fallback={
                            <div class="file-viewer-empty">
                              <span class="file-viewer-empty-text">{props.t("instanceInfo.loading")}</span>
                            </div>
                          }
                        >
                          <LazyMonacoFileViewer
                            scopeKey={props.scopeKey()}
                            path={payload().path}
                            content={payload().content}
                            onSave={props.onSave}
                            onContentChange={props.onContentChange}
                          />
                        </Suspense>
                      )}
                    </Show>
                  }
                >
                  {(err) => (
                    <div class="file-viewer-empty">
                      <span class="file-viewer-empty-text">{err()}</span>
                    </div>
                  )}
                </Show>
              }
            >
              <div class="file-viewer-empty">
                <span class="file-viewer-empty-text">{props.t("instanceInfo.loading")}</span>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </div>
    )
  }

  const renderContent = (): JSX.Element => {
    const parent = props.parentPath()

    return (
      <div class="files-tab-container">
        <div class="files-tab-header">
          <div class="files-tab-header-row">
            <div class="files-tab-stats">
              <span class="files-tab-selected-path" title={headerDisplayedPath()}>
                <span class="file-path-text">{headerDisplayedPath()}</span>
              </span>
              <Show when={props.browserLoading()}>
                <span>{props.t("instanceInfo.loading")}</span>
              </Show>
              <Show when={props.browserError()}>{(err) => <span class="text-error">{err()}</span>}</Show>
            </div>
            <button
              type="button"
              class="files-header-icon-button"
              title={props.t("instanceShell.rightPanel.actions.refresh")}
              aria-label={props.t("instanceShell.rightPanel.actions.refresh")}
              disabled={props.browserLoading()}
              onClick={() => props.onRefresh()}
            >
              <RefreshCw class={`h-4 w-4${props.browserLoading() ? " animate-spin" : ""}`} />
            </button>
          </div>
        </div>

        <div class="files-tab-body">
          <div class="file-list-scroll flex-1">
            <Show when={parent}>
              {(p) => (
                <div class="file-list-item" onClick={() => props.onLoadEntries(p())}>
                  <div class="file-list-item-content">
                    <div class="file-list-item-path" title={p()}>
                      <span class="file-path-text">..</span>
                    </div>
                  </div>
                </div>
              )}
            </Show>

            <Show when={props.browserLoading() && props.browserEntries() === null}>
              <div class="p-3 text-xs text-secondary">{props.t("instanceInfo.loading")}</div>
            </Show>

            <For each={sorted()}>
              {(item) => (
                <div
                  class={`file-list-item ${props.browserSelectedPath() === item.path ? "file-list-item-active" : ""}`}
                  onClick={() => {
                    if (item.type === "directory") {
                      props.onLoadEntries(item.path)
                      return
                    }
                    handleFileClick(item.path)
                  }}
                  title={item.path}
                >
                  <div class="file-list-item-content">
                    <div class="file-list-item-path" title={item.path}>
                      <span class="file-path-text">{item.name}</span>
                    </div>
                    <div class="file-list-item-stats">
                      <span class="text-[10px] text-secondary">{item.type}</span>
                    </div>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>

        <Dialog open={modalOpen() && !!props.browserSelectedPath()} onOpenChange={(open) => setModalOpen(open)}>
          <Dialog.Portal>
            <Dialog.Overlay class="modal-overlay" />
            <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
              <Dialog.Content class="outline-none">{renderModalContent()}</Dialog.Content>
            </div>
          </Dialog.Portal>
        </Dialog>
      </div>
    )
  }

  return <>{renderContent()}</>
}

export default FilesTab
