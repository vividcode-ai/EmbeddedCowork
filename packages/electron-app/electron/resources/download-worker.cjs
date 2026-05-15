const { createWriteStream, writeFileSync, statSync } = require("fs")
const { rename, unlink, chmod } = require("fs/promises")
const path = require("path")

const MAX_RETRIES = 5
const RETRY_DELAYS = [1000, 2000, 4000, 8000, 16000]

const PROXY_PREFIXES = [
  "https://ghproxy.net/",
  "https://ghproxy.com/",
  "https://gitproxy.click/",
  "https://githubproxy.cc/",
  "https://hub.fastgit.org/",
  "https://hub.nuaa.cf/",
  "https://hub.yzuu.cf/",
  "https://bgithub.xyz/",
  "https://github.wuyanzheshui.workers.dev/",
]

async function tryDownloadWithResume(url, dest) {
  const tmpDest = dest + ".tmp"

  for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
    let existingSize = 0
    try {
      existingSize = statSync(tmpDest).size
    } catch {}

    let writer = null
    try {
      const headers = existingSize > 0 ? { Range: `bytes=${existingSize}-` } : {}
      const res = await fetch(url, { headers })

      if (res.status === 416) {
        await rename(tmpDest, dest)
        return true
      }

      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`)

      if (existingSize > 0 && res.status !== 206) {
        try { await unlink(tmpDest) } catch {}
        existingSize = 0
      }

      writer = createWriteStream(tmpDest, existingSize > 0 ? { flags: "a" } : { flags: "w" })
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
      return true
    } catch (err) {
      if (writer) {
        writer.destroy()
        writer = null
      }

      if (attempt < MAX_RETRIES - 1) {
        const delay = RETRY_DELAYS[attempt] ?? 16000
        await new Promise((r) => setTimeout(r, delay))
      }
    }
  }

  return false
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

const allUrls = [url, ...PROXY_PREFIXES.map((p) => p + url)]

;(async () => {
  let lastSource = ""
  for (const attemptUrl of allUrls) {
    const sourceLabel = attemptUrl === url ? "direct" : new URL(attemptUrl).hostname
    console.error(`[download-worker] trying ${sourceLabel}...`)
    if (await tryDownloadWithResume(attemptUrl, destPath)) {
      console.error(`[download-worker] succeeded via ${sourceLabel}`)
      writeFileSync(pkgPath, JSON.stringify({ version }, null, 2), "utf-8")
      process.exit(0)
    }
    lastSource = sourceLabel
  }
  console.error(`[download-worker] all sources failed, last: ${lastSource}`)
  process.exit(1)
})()
