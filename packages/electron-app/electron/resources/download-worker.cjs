const { createWriteStream, writeFileSync } = require("fs")
const { rename, unlink, chmod } = require("fs/promises")
const path = require("path")

const RETRY_DELAYS = [1000, 2000, 4000, 8000, 16000]

async function download(url, dest) {
  const tmpDest = dest + ".tmp"

  for (let attempt = 0; attempt < RETRY_DELAYS.length; attempt++) {
    let writer = null
    try {
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`)

      writer = createWriteStream(tmpDest)
      const reader = res.body.getReader()

      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        writer.write(value)
      }

      await new Promise((resolve, reject) => {
        writer.end((err) => (err ? reject(err) : resolve()))
      })

      if (process.platform !== "win32") {
        await chmod(tmpDest, 0o755)
      }

      await rename(tmpDest, dest)
      return 0
    } catch (err) {
      if (writer) {
        writer.destroy()
        writer = null
      }
      try {
        await unlink(tmpDest)
      } catch {}

      if (attempt === RETRY_DELAYS.length - 1) {
        console.error(`[download-worker] failed after ${RETRY_DELAYS.length} attempts:`, err.message)
        return 1
      }

      await new Promise((r) => setTimeout(r, RETRY_DELAYS[attempt]))
    }
  }

  return 1
}

const url = process.argv[2]
const serverDir = process.argv[3]
const version = process.argv[4]

if (!url || !serverDir || !version) {
  console.error("Usage: download-worker.cjs <url> <server-dir> <version>")
  process.exit(1)
}

const ext = process.platform === "win32" ? ".exe" : ""
const binaryName = `embeddedcowork-server-${version}${ext}`
const destPath = path.join(serverDir, binaryName)
const pkgPath = path.join(serverDir, "package.json")

download(url, destPath).then((code) => {
  if (code === 0) {
    writeFileSync(pkgPath, JSON.stringify({ version }, null, 2), "utf-8")
  }
  process.exit(code)
})
