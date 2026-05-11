#!/usr/bin/env node
const { spawn } = require("child_process")
const path = require("path")
const fs = require("fs")

const platformMap = {
  "darwin-x64": "@vividcodeai/embeddedcowork-darwin-x64",
  "darwin-arm64": "@vividcodeai/embeddedcowork-darwin-arm64",
  "win32-x64": "@vividcodeai/embeddedcowork-win32-x64",
  "linux-x64": "@vividcodeai/embeddedcowork-linux-x64",
}

const key = `${process.platform}-${process.arch}`
const pkgName = platformMap[key]

if (!pkgName) {
  console.error(`Unsupported platform: ${key}`)
  console.error("Supported platforms: darwin-x64, darwin-arm64, win32-x64, linux-x64")
  process.exit(1)
}

const binaryName = process.platform === "win32" ? "embeddedcowork-server.exe" : "embeddedcowork-server"

function findBinary(startDir) {
  let current = startDir
  for (;;) {
    const modules = path.join(current, "node_modules")
    if (fs.existsSync(modules)) {
      const candidate = path.join(modules, pkgName, "bin", binaryName)
      if (fs.existsSync(candidate)) return candidate
    }
    const parent = path.dirname(current)
    if (parent === current) return null
    current = parent
  }
}

const scriptDir = path.dirname(fs.realpathSync(__filename))
const binPath = findBinary(scriptDir)

if (!binPath) {
  console.error(
    `Failed to find binary for platform ${key}. Try reinstalling: npm install @vividcodeai/embeddedcowork`
  )
  process.exit(1)
}

const child = spawn(binPath, process.argv.slice(2), { stdio: "inherit" })
child.on("exit", (code, signal) => {
  if (signal === "SIGTERM" && code === null) {
    process.exit(0)
  }
  process.exit(code ?? 1)
})
process.on("SIGTERM", () => child.kill("SIGTERM"))
process.on("SIGINT", () => child.kill("SIGINT"))
