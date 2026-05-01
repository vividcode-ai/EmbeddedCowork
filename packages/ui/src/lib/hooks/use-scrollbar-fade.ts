import { createSignal, onCleanup, type Accessor } from "solid-js"

const DEFAULT_DELAY = 500
const CSS_VAR = "--sb-thumb-color"

export function useScrollbarFade(
  el: Accessor<HTMLElement | undefined>,
  delay = DEFAULT_DELAY,
) {
  const [isHovered, setIsHovered] = createSignal(false)

  let hideTimer: ReturnType<typeof setTimeout> | undefined
  let rgbCache = ""

  function resolveRgb(): string {
    if (rgbCache) return rgbCache
    const element = el()
    if (!element) return ""
    const style = getComputedStyle(element)
    const value = style.getPropertyValue("--border-muted").trim()
    if (!value) return ""
    const div = document.createElement("div")
    div.style.color = value
    div.style.display = "none"
    document.body.appendChild(div)
    const parsed = getComputedStyle(div).color
    document.body.removeChild(div)
    const m = parsed.match(/^rgb\((\d+),\s*(\d+),\s*(\d+)\)$/)
    if (!m) return ""
    rgbCache = `${m[1]}, ${m[2]}, ${m[3]}`
    return rgbCache
  }

  function setAlpha(alpha: number) {
    const element = el()
    if (!element) return
    const rgb = resolveRgb()
    if (!rgb) return
    element.style.setProperty(CSS_VAR, `rgba(${rgb}, ${alpha})`)
  }

  function cancelPending() {
    if (hideTimer !== undefined) { clearTimeout(hideTimer); hideTimer = undefined }
  }

  function handleMouseEnter() {
    cancelPending()
    setAlpha(1)
    setIsHovered(true)
  }

  function handleMouseLeave() {
    cancelPending()
    hideTimer = setTimeout(() => {
      setAlpha(0)
      setIsHovered(false)
    }, delay)
  }

  function dispose() {
    cancelPending()
    const element = el()
    if (element) element.style.removeProperty(CSS_VAR)
  }

  onCleanup(dispose)

  return { isHovered, handleMouseEnter, handleMouseLeave, dispose }
}
