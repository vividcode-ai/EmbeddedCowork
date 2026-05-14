import { createEffect, createSignal, onCleanup, onMount } from "solid-js"
import { renderMarkdown, initMarkdown, setMarkdownTheme } from "../../lib/markdown"
import { useTheme } from "../../lib/theme"

interface MarkdownFileViewerProps {
  content: string
  onSave?: () => void
}

export function MarkdownFileViewer(props: MarkdownFileViewerProps) {
  const { isDark } = useTheme()
  let host: HTMLDivElement | undefined
  let html = ""

  const handleKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault()
      props.onSave?.()
    }
  }

  onMount(() => {
    void (async () => {
      await initMarkdown(isDark())
      html = await renderMarkdown(props.content, { escapeRawHtml: true })
      if (host) {
        host.innerHTML = html
      }
    })()

    document.addEventListener("keydown", handleKeyDown)
    onCleanup(() => {
      document.removeEventListener("keydown", handleKeyDown)
    })
  })

  createEffect(() => {
    setMarkdownTheme(isDark())
  })

  createEffect(() => {
    void (async () => {
      html = await renderMarkdown(props.content, { escapeRawHtml: true })
      if (host) {
        host.innerHTML = html
      }
    })()
  })

  return (
    <div
      class="markdown-preview markdown-body"
      ref={host}
    />
  )
}
