#!/usr/bin/env node

const fs = require("fs")
const path = require("path")
const { execSync } = require("child_process")
const { pathToFileURL } = require("url")

const root = path.resolve(__dirname, "..")
const workspaceRoot = path.resolve(root, "..", "..")
const uiRoot = path.resolve(root, "..", "ui")
const uiDist = path.resolve(uiRoot, "src", "renderer", "dist")
const uiLoadingDest = path.resolve(root, "src-tauri", "resources", "ui-loading")

async function ensureMonacoAssets() {
  const helperPath = path.join(uiRoot, "scripts", "monaco-public-assets.js")
  const helperUrl = pathToFileURL(helperPath).href
  const { copyMonacoPublicAssets } = await import(helperUrl)
  copyMonacoPublicAssets({
    uiRendererRoot: path.join(uiRoot, "src", "renderer"),
    warn: (msg) => console.warn(`[dev-prep] ${msg}`),
    sourceRoots: [
      path.resolve(workspaceRoot, "node_modules", "monaco-editor", "min", "vs"),
      path.resolve(uiRoot, "node_modules", "monaco-editor", "min", "vs"),
    ],
  })
}

function ensureUiBuild() {
  const loadingHtml = path.join(uiDist, "loading.html")
  if (fs.existsSync(loadingHtml)) {
    return
  }

  console.log("[dev-prep] UI loader build missing; running workspace build…")
  execSync("npm --workspace @embeddedcowork/ui run build", {
    cwd: workspaceRoot,
    stdio: "inherit",
  })

  if (!fs.existsSync(loadingHtml)) {
    throw new Error("[dev-prep] failed to produce loading.html after UI build")
  }
}

function copyUiLoadingAssets() {
  const loadingSource = path.join(uiDist, "loading.html")
  const assetsSource = path.join(uiDist, "assets")

  fs.rmSync(uiLoadingDest, { recursive: true, force: true })
  fs.mkdirSync(uiLoadingDest, { recursive: true })

  fs.copyFileSync(loadingSource, path.join(uiLoadingDest, "loading.html"))
  if (fs.existsSync(assetsSource)) {
    fs.cpSync(assetsSource, path.join(uiLoadingDest, "assets"), { recursive: true })
  }

  console.log(`[dev-prep] copied loader bundle from ${uiDist}`)
}

const RUST_TARGETS = {
  "win32-x64": "x86_64-pc-windows-msvc",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
}

function ensureSidecar() {
  const platformKey = `${process.platform}-${process.arch}`
  const targetTriple = RUST_TARGETS[platformKey]
  if (!targetTriple) return

  const serverRoot = path.resolve(root, "..", "server")
  const ext = process.platform === "win32" ? ".exe" : ""
  const sidecarName = `embeddedcowork-server-${targetTriple}${ext}`
  const sidecarsDir = path.resolve(root, "src-tauri", "sidecars")
  const dest = path.join(sidecarsDir, sidecarName)

  if (fs.existsSync(dest)) {
    console.log(`[dev-prep] sidecar already exists: ${dest}`)
    return
  }

  const builtBinary = path.join(serverRoot, "dist", `embeddedcowork-server${ext}`)
  if (fs.existsSync(builtBinary)) {
    fs.mkdirSync(sidecarsDir, { recursive: true })
    fs.cpSync(builtBinary, dest)
    console.log(`[dev-prep] copied sidecar from existing build: ${dest}`)
    return
  }

  try {
    console.log("[dev-prep] building standalone server executable...")
    execSync("npm run build:standalone --workspace @vividcodeai/embeddedcowork", {
      cwd: workspaceRoot,
      stdio: "inherit",
      env: {
        ...process.env,
        PATH: `${path.join(workspaceRoot, "node_modules/.bin")}${path.delimiter}${process.env.PATH ?? ""}`,
      },
    })
    if (fs.existsSync(builtBinary)) {
      fs.mkdirSync(sidecarsDir, { recursive: true })
      fs.cpSync(builtBinary, dest)
      console.log(`[dev-prep] built and copied sidecar: ${dest}`)
      return
    }
  } catch (e) {
    console.warn("[dev-prep] standalone build failed; creating placeholder sidecar")
  }

  fs.mkdirSync(sidecarsDir, { recursive: true })
  fs.writeFileSync(dest, "")
  console.log(`[dev-prep] created placeholder sidecar for compilation: ${dest}`)
}

function ensureServerResources() {
  const dest = path.resolve(root, "src-tauri", "resources", "server")
  if (fs.existsSync(dest)) return

  console.log("[dev-prep] creating placeholder resources/server directory")
  fs.mkdirSync(dest, { recursive: true })
  fs.writeFileSync(path.join(dest, "package.json"), JSON.stringify({ name: "placeholder" }))
}

;(async () => {
  await ensureMonacoAssets()
  ensureUiBuild()
  copyUiLoadingAssets()
  ensureSidecar()
  ensureServerResources()
})().catch((err) => {
  console.error("[dev-prep] failed:", err)
  process.exit(1)
})
