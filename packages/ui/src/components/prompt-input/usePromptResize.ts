import { createSignal, onCleanup } from "solid-js"

interface UsePromptResizeOptions {
  getTextarea: () => HTMLTextAreaElement | null
  minHeight?: number
  maxHeight?: number
}

export function usePromptResize(options: UsePromptResizeOptions) {
  const minHeight = options.minHeight ?? 56
  const maxHeight = options.maxHeight ?? 400

  const [isResizing, setIsResizing] = createSignal(false)

  function onResizeHandlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return
    e.preventDefault()

    const textarea = options.getTextarea()
    if (!textarea) return

    const startY = e.clientY
    const startHeight = textarea.offsetHeight
    let currentHeight = startHeight

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaY = startY - moveEvent.clientY
      currentHeight = Math.min(maxHeight, Math.max(minHeight, startHeight + deltaY))
      textarea.style.height = `${currentHeight}px`
      setIsResizing(true)
    }

    function onPointerUp() {
      document.removeEventListener("pointermove", onPointerMove)
      document.removeEventListener("pointerup", onPointerUp)
      document.removeEventListener("pointercancel", onPointerUp)
      document.body.style.userSelect = ""
      document.body.style.cursor = ""
      setIsResizing(false)
    }

    document.addEventListener("pointermove", onPointerMove)
    document.addEventListener("pointerup", onPointerUp)
    document.addEventListener("pointercancel", onPointerUp)
    document.body.style.userSelect = "none"
    document.body.style.cursor = "ns-resize"

    onCleanup(() => {
      document.removeEventListener("pointermove", onPointerMove)
      document.removeEventListener("pointerup", onPointerUp)
      document.removeEventListener("pointercancel", onPointerUp)
      document.body.style.userSelect = ""
      document.body.style.cursor = ""
    })
  }

  return {
    isResizing,
    onResizeHandlePointerDown,
  }
}
