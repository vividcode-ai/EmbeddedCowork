import { existsSync, rmSync } from "fs"
import { join } from "path"

const appData = process.env.APPDATA
if (!appData) {
  console.log("[clear-dev-cache] APPDATA not found, skipping")
  process.exit(0)
}

const sessionDataDir = join(appData, "EmbeddedCowork", "session-data")
if (!existsSync(sessionDataDir)) {
  console.log("[clear-dev-cache] session-data not found, skipping")
  process.exit(0)
}

const cleanDirs = [
  "Cache",
  "Code Cache",
  "GPUCache",
  "DawnGraphiteCache",
  "DawnWebGPUCache",
  "blob_storage",
  "Shared Dictionary",
]

let cleaned = 0
for (const dir of cleanDirs) {
  const fullPath = join(sessionDataDir, dir)
  if (existsSync(fullPath)) {
    rmSync(fullPath, { recursive: true, force: true })
    console.log(`[clear-dev-cache] cleaned: ${dir}`)
    cleaned++
  }
}

if (cleaned === 0) {
  console.log("[clear-dev-cache] nothing to clean")
} else {
  console.log(`[clear-dev-cache] done, cleaned ${cleaned} director${cleaned > 1 ? "ies" : "y"}`)
}
