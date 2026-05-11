#!/usr/bin/env node
import fs from "fs"
import path from "path"
import { spawnSync } from "child_process"
import { fileURLToPath } from "url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const cliRoot = path.resolve(__dirname, "..")
const distDir = path.join(cliRoot, "dist")
const publicDir = path.join(cliRoot, "public")
const authPagesSourceDir = path.join(distDir, "server", "routes", "auth-pages")
const authPagesTargetDir = path.join(distDir, "auth-pages")
const packageJsonPath = path.join(cliRoot, "package.json")
const publishDir = path.join(cliRoot, "publish-packages")

const targets = [
  { key: "darwin-x64",   bunTarget: "bun-darwin-x64"   },
  { key: "darwin-arm64", bunTarget: "bun-darwin-arm64" },
  { key: "win32-x64",    bunTarget: "bun-windows-x64"  },
  { key: "linux-x64",    bunTarget: "bun-linux-x64"    },
]

function resolveBunCommand() {
  const executableName = process.platform === "win32" ? "bun.exe" : "bun"
  const localBinName = process.platform === "win32" ? "bun.cmd" : "bun"
  const candidates = [
    path.join(cliRoot, "node_modules", ".bin", localBinName),
    path.join(cliRoot, "..", "..", "node_modules", ".bin", localBinName),
    path.join(cliRoot, "node_modules", "bun", "bin", executableName),
    path.join(cliRoot, "..", "..", "node_modules", "bun", "bin", executableName),
  ]

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate
    }
  }

  return "bun"
}

function fail(message) {
  console.error(`[build-platforms] ${message}`)
  process.exit(1)
}

function ensureArtifacts() {
  const requiredPaths = [distDir, publicDir, authPagesSourceDir, packageJsonPath]
  const missing = requiredPaths.filter((filePath) => !fs.existsSync(filePath))
  if (missing.length > 0) {
    fail(`Missing required build artifacts: ${missing.join(", ")}. Run npm run build first.`)
  }

  const bunResult = spawnSync(resolveBunCommand(), ["-v"], { cwd: cliRoot, encoding: "utf-8", shell: process.platform === "win32" })
  if (bunResult.status !== 0) {
    fail("Bun is required to build the standalone server executable. Install dependencies so the local Bun binary is available.")
  }
}

function syncStandaloneAuthPages() {
  fs.rmSync(authPagesTargetDir, { recursive: true, force: true })
  fs.mkdirSync(path.dirname(authPagesTargetDir), { recursive: true })
  fs.cpSync(authPagesSourceDir, authPagesTargetDir, { recursive: true })
}

function buildForTarget(target) {
  const { key, bunTarget } = target
  const isWindows = bunTarget.includes("windows")
  const outputName = isWindows ? "embeddedcowork-server.exe" : "embeddedcowork-server"
  const outputPath = path.join(distDir, outputName)
  const platformBinDir = path.join(publishDir, key, "bin")
  const platformBinPath = path.join(platformBinDir, outputName)

  fs.rmSync(outputPath, { force: true })
  const bunCommand = resolveBunCommand()

  const args = ["build", "--compile", `--target=${bunTarget}`]
  args.push(path.join(cliRoot, "src", "index.ts"), "--outfile", outputPath)

  console.log(`[build-platforms] Building for ${key} (${bunTarget})...`)
  const result = spawnSync(bunCommand, args, {
    cwd: cliRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
  })

  if (result.status !== 0) {
    if (result.error) throw result.error
    throw new Error(`bun build --compile for ${key} exited with code ${result.status ?? 1}`)
  }

  // Copy to platform package bin/
  fs.mkdirSync(platformBinDir, { recursive: true })
  fs.cpSync(outputPath, platformBinPath)
  console.log(`[build-platforms] Copied to ${platformBinPath}`)

  // Set executable permission on Unix
  if (!isWindows) {
    try { fs.chmodSync(platformBinPath, 0o755) } catch {}
  }
}

function main() {
  ensureArtifacts()
  syncStandaloneAuthPages()

  const results = []
  for (const target of targets) {
    try {
      buildForTarget(target)
      results.push({ key: target.key, status: "ok" })
    } catch (error) {
      console.error(`[build-platforms] Skipping ${target.key}: ${error.message}`)
      results.push({ key: target.key, status: "skipped" })
    }
  }

  const ok = results.filter((r) => r.status === "ok").length
  const skipped = results.filter((r) => r.status === "skipped").length
  console.log(`[build-platforms] Done: ${ok} built, ${skipped} skipped`)
  if (skipped > 0) {
    console.log(`[build-platforms] Skipped: ${results.filter((r) => r.status === "skipped").map((r) => r.key).join(", ")}`)
  }
}

try {
  main()
} catch (error) {
  console.error("[build-platforms] failed:", error)
  process.exit(1)
}
