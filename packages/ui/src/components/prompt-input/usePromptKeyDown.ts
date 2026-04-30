import type { Accessor } from "solid-js"
import type { Attachment } from "../../types/attachment"
import type { ExpandState, PromptMode } from "./types"

export type UsePromptKeyDownOptions = {
  getTextarea: () => HTMLTextAreaElement | null

  prompt: Accessor<string>
  setPrompt: (v: string) => void

  mode: Accessor<PromptMode>
  setMode: (m: PromptMode) => void

  isPickerOpen: Accessor<boolean>
  closePicker: () => void

  ignoredAtPositions: Accessor<Set<number>>
  setIgnoredAtPositions: (next: Set<number> | ((s: Set<number>) => Set<number>)) => void

  getAttachments: Accessor<Attachment[]>
  removeAttachment: (attachmentId: string) => void

  submitOnEnter: Accessor<boolean>
  onSend: () => void

  selectPreviousHistory: (force?: boolean) => boolean
  selectNextHistory: (force?: boolean) => boolean

  expandState: Accessor<ExpandState>
  onToggleExpand: (next: ExpandState) => void
  onAutoGrow?: () => void
}

export function usePromptKeyDown(options: UsePromptKeyDownOptions) {
  const insertNewlineAtCursor = () => {
    const textarea = options.getTextarea()
    const current = options.prompt()
    const start = textarea ? textarea.selectionStart : current.length
    const end = textarea ? textarea.selectionEnd : current.length
    const nextValue = current.substring(0, start) + "\n" + current.substring(end)
    const nextCursor = start + 1

    options.setPrompt(nextValue)
    options.onAutoGrow?.()

    setTimeout(() => {
      const nextTextarea = options.getTextarea()
      if (!nextTextarea) return
      nextTextarea.focus()
      nextTextarea.setSelectionRange(nextCursor, nextCursor)
    }, 0)
  }

  return function handleKeyDown(e: KeyboardEvent) {
    const textarea = options.getTextarea()
    if (!textarea) return

    const currentText = options.prompt()
    const cursorAtBufferStart = textarea.selectionStart === 0 && textarea.selectionEnd === 0
    const isShellMode = options.mode() === "shell"

    // Ctrl/Cmd + ArrowUp → toggle expand state
    if ((e.metaKey || e.ctrlKey) && e.key === "ArrowUp") {
      e.preventDefault()
      const next = options.expandState() === "normal" ? "expanded" : "normal"
      options.onToggleExpand(next)
      return
    }

    if (!isShellMode && e.key === "!" && cursorAtBufferStart && currentText.length === 0 && !textarea.disabled) {
      e.preventDefault()
      options.setMode("shell")
      return
    }

    if (options.isPickerOpen() && e.key === "Escape") {
      e.preventDefault()
      e.stopPropagation()
      options.closePicker()
      return
    }

    if (isShellMode) {
      if (e.key === "Escape") {
        e.preventDefault()
        e.stopPropagation()
        options.setMode("normal")
        return
      }
      if (e.key === "Backspace" && cursorAtBufferStart && currentText.length === 0) {
        e.preventDefault()
        options.setMode("normal")
        return
      }
    }

    if (e.key === "Backspace" || e.key === "Delete") {
      const cursorPos = textarea.selectionStart
      const text = currentText

      const pastePlaceholderRegex = createPastedPlaceholderRegex()
      let pasteMatch

      while ((pasteMatch = pastePlaceholderRegex.exec(text)) !== null) {
        const placeholderStart = pasteMatch.index
        const placeholderEnd = pasteMatch.index + pasteMatch[0].length
        const pasteNumber = pasteMatch[1]

        const isDeletingFromEnd = e.key === "Backspace" && cursorPos === placeholderEnd
        const isDeletingFromStart = e.key === "Delete" && cursorPos === placeholderStart
        const isSelected =
          textarea.selectionStart <= placeholderStart &&
          textarea.selectionEnd >= placeholderEnd &&
          textarea.selectionStart !== textarea.selectionEnd

        if (isDeletingFromEnd || isDeletingFromStart || isSelected) {
          e.preventDefault()

          const currentAttachments = options.getAttachments()
          const attachment = currentAttachments.find(
            (a) => a.source.type === "text" && a.display.includes(`pasted #${pasteNumber}`),
          )

          if (attachment) {
            options.removeAttachment(attachment.id)
          }

          const newText = text.substring(0, placeholderStart) + text.substring(placeholderEnd)
          options.setPrompt(newText)

          setTimeout(() => {
            textarea.setSelectionRange(placeholderStart, placeholderStart)
          }, 0)

          return
        }
      }

      const imagePlaceholderRegex = createImagePlaceholderRegex()
      let imageMatch

      while ((imageMatch = imagePlaceholderRegex.exec(text)) !== null) {
        const placeholderStart = imageMatch.index
        const placeholderEnd = imageMatch.index + imageMatch[0].length
        const imageNumber = imageMatch[1]

        const isDeletingFromEnd = e.key === "Backspace" && cursorPos === placeholderEnd
        const isDeletingFromStart = e.key === "Delete" && cursorPos === placeholderStart
        const isSelected =
          textarea.selectionStart <= placeholderStart &&
          textarea.selectionEnd >= placeholderEnd &&
          textarea.selectionStart !== textarea.selectionEnd

        if (isDeletingFromEnd || isDeletingFromStart || isSelected) {
          e.preventDefault()

          const currentAttachments = options.getAttachments()
          const attachment = currentAttachments.find(
            (a) => a.source.type === "file" && a.mediaType.startsWith("image/") && a.display.includes(`Image #${imageNumber}`),
          )

          if (attachment) {
            options.removeAttachment(attachment.id)
          }

          const newText = text.substring(0, placeholderStart) + text.substring(placeholderEnd)
          options.setPrompt(newText)

          setTimeout(() => {
            textarea.setSelectionRange(placeholderStart, placeholderStart)
          }, 0)

          return
        }
      }

      const mentionRegex = createMentionRegex()
      let mentionMatch

      while ((mentionMatch = mentionRegex.exec(text)) !== null) {
        const mentionStart = mentionMatch.index
        const mentionEnd = mentionMatch.index + mentionMatch[0].length
        const name = mentionMatch[1]

        const isDeletingFromEnd = e.key === "Backspace" && cursorPos === mentionEnd
        const isDeletingFromStart = e.key === "Delete" && cursorPos === mentionStart
        const isSelected =
          textarea.selectionStart <= mentionStart &&
          textarea.selectionEnd >= mentionEnd &&
          textarea.selectionStart !== textarea.selectionEnd

        if (isDeletingFromEnd || isDeletingFromStart || isSelected) {
          const currentAttachments = options.getAttachments()
          const attachment = currentAttachments.find((a) => {
            if (a.source.type === "agent") {
              return a.filename === name
            }
            if (a.source.type === "file") {
              // Match either by filename (basename) or by path (for full paths like @docs/file.txt)
              return (
                a.filename === name ||
                a.source.path === name ||
                a.source.path.endsWith("/" + name) ||
                a.source.path === name.replace(/\/$/, "")
              )
            }
            if (a.source.type === "text") {
              // For text attachments (path-only mentions), match by value
              return a.source.value === name || a.source.value.endsWith("/" + name)
            }
            return false
          })

          if (attachment) {
            e.preventDefault()

            options.removeAttachment(attachment.id)

            options.setIgnoredAtPositions((prev) => {
              const next = new Set(prev)
              next.delete(mentionStart)
              return next
            })

            const newText = text.substring(0, mentionStart) + text.substring(mentionEnd)
            options.setPrompt(newText)

            setTimeout(() => {
              textarea.setSelectionRange(mentionStart, mentionStart)
            }, 0)

            // Check if there are any @ remaining in the text - if not, close the picker
            if (!newText.includes("@") && options.isPickerOpen()) {
              options.closePicker()
              // Clear ignoredAtPositions since we deleted the entire @mention
              // This ensures typing @ again will open the picker
              options.setIgnoredAtPositions(new Set())
            }

            return
          }
        }
      }
    }

    if (e.key === "Enter") {
      const isModified = e.metaKey || e.ctrlKey

      // If the picker is open, Enter should select from it.
      if (!isModified && options.isPickerOpen()) {
        return
      }

      if (options.submitOnEnter()) {
        // Swapped mode: Enter submits, Cmd/Ctrl+Enter inserts a newline.
        if (isModified) {
          e.preventDefault()
          e.stopPropagation()
          insertNewlineAtCursor()
          return
        }

        // Shift+Enter always inserts a newline.
        if (e.shiftKey) {
          // If the picker is open, avoid selecting an item on Enter.
          if (options.isPickerOpen()) {
            e.stopPropagation()
          }
          return
        }

        e.preventDefault()
        options.onSend()
        return
      }

      // Default: Cmd/Ctrl+Enter submits.
      if (isModified) {
        e.preventDefault()
        if (options.isPickerOpen()) {
          options.closePicker()
        }
        options.onSend()
        return
      }
    }

    if (e.key === "PageUp") {
      if (currentText.length === 0) {
        e.preventDefault()
        const handled = options.selectPreviousHistory(true)
        if (handled) return
      } else {
        e.preventDefault()
        const pos = textarea.selectionStart
        const textBefore = currentText.substring(0, pos)
        const lineStart = textBefore.lastIndexOf("\n") + 1
        if (lineStart === 0) return
        const col = pos - lineStart
        const textBeforeLine = currentText.substring(0, lineStart - 1)
        const prevLineStart = textBeforeLine.lastIndexOf("\n") + 1
        const prevLine = currentText.substring(prevLineStart, lineStart - 1)
        const targetCol = Math.min(col, prevLine.length)
        textarea.setSelectionRange(prevLineStart + targetCol, prevLineStart + targetCol)
        return
      }
    }

    if (e.key === "PageDown") {
      if (currentText.length === 0) {
        e.preventDefault()
        const handled = options.selectNextHistory(true)
        if (handled) return
      } else {
        e.preventDefault()
        const pos = textarea.selectionStart
        const textBefore = currentText.substring(0, pos)
        const lineStart = textBefore.lastIndexOf("\n") + 1
        const col = pos - lineStart
        const afterLineStart = currentText.substring(lineStart)
        const currentLineNewline = afterLineStart.indexOf("\n")
        const lineEndPos = currentLineNewline >= 0 ? lineStart + currentLineNewline : currentText.length
        const nextLineStart = lineEndPos + 1
        if (nextLineStart >= currentText.length) return
        const afterNextLine = currentText.substring(nextLineStart)
        const nextLineNewline = afterNextLine.indexOf("\n")
        const nextLineEnd = nextLineNewline >= 0 ? nextLineStart + nextLineNewline : currentText.length
        const nextLine = currentText.substring(nextLineStart, nextLineEnd)
        const targetCol = Math.min(col, nextLine.length)
        textarea.setSelectionRange(nextLineStart + targetCol, nextLineStart + targetCol)
        return
      }
    }

    if (e.key === "ArrowUp") {
      const handled = options.selectPreviousHistory()
      if (handled) {
        e.preventDefault()
        return
      }
    }

    if (e.key === "ArrowDown") {
      const handled = options.selectNextHistory()
      if (handled) {
        e.preventDefault()
        return
      }
    }
  }
}
