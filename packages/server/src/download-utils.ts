import { createWriteStream } from "fs"
import { unlink } from "fs/promises"

export interface DownloadOptions {
  progressCb?: (current: number, total: number) => void
  retries?: number
  signal?: AbortSignal
}

export async function downloadFileWithRetry(
  url: string,
  dest: string,
  options?: DownloadOptions,
): Promise<void> {
  const retries = options?.retries ?? 5
  const { progressCb, signal } = options ?? {}

  for (let attempt = 1; attempt <= retries; attempt++) {
    let writer: import("fs").WriteStream | null = null
    try {
      const res = await fetch(url, { signal })
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`)

      const total = Number(res.headers.get("content-length") ?? 0)
      let current = 0

      const reader = res.body?.getReader()
      if (!reader) throw new Error("Response body is not readable")

      writer = createWriteStream(dest)

      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        current += value.length
        progressCb?.(current, total)
        writer.write(value)
      }
      await new Promise<void>((resolve, reject) => {
        writer!.end((err: Error | null) => (err ? reject(err) : resolve()))
      })

      return
    } catch (err) {
      if (writer) {
        writer.destroy()
        writer = null
      }
      try { await unlink(dest) } catch {}
      if (attempt === retries) throw err
      const delay = Math.min(1000 * Math.pow(2, attempt - 1), 30000)
      await new Promise((r) => setTimeout(r, delay))
    }
  }
}
