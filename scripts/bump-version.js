#!/usr/bin/env node

const { spawnSync } = require("child_process")
const fs = require("fs")
const path = require("path")

const versionArgs = process.argv.slice(2)

if (versionArgs.length === 0) {
  console.error("[bumpVersion] missing version argument (example: npm run bumpVersion patch/minor/-major)")
  process.exit(1)
}

const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm"

function runStep(args, label) {
  const result = spawnSync(npmCommand, args, {
    stdio: "inherit",
     shell: process.platform === "win32",
  })

  if (result.error) {
    console.error(`[bumpVersion] failed during ${label}: ${result.error.message}`)
    process.exit(1)
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

runStep(
  [
    "version",
    ...versionArgs,
    "--workspaces",
    "--include-workspace-root",
    "--no-git-tag-version",
  ],
  "npm version"
)

const rootPkgPath = path.join(__dirname, "..", "package.json")
const serverPkgPath = path.join(__dirname, "..", "packages", "server", "package.json")
const rootPkg = JSON.parse(fs.readFileSync(rootPkgPath, "utf-8"))
const serverPkg = JSON.parse(fs.readFileSync(serverPkgPath, "utf-8"))
if (serverPkg.optionalDependencies) {
  for (const depKey of Object.keys(serverPkg.optionalDependencies)) {
    if (depKey.startsWith("@vividcodeai/embeddedcowork-")) {
      serverPkg.optionalDependencies[depKey] = rootPkg.version
    }
  }
  fs.writeFileSync(serverPkgPath, JSON.stringify(serverPkg, null, 2) + "\n")
  console.log(`[bumpVersion] Synced optionalDependencies to ${rootPkg.version}`)
}

const uiPkg = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "packages", "ui", "package.json"), "utf-8"))
const publicUiVersionPath = path.join(__dirname, "..", "packages", "server", "public", "ui-version.json")
if (fs.existsSync(publicUiVersionPath)) {
  fs.writeFileSync(publicUiVersionPath, JSON.stringify({ uiVersion: uiPkg.version }, null, 2) + "\n")
  console.log(`[bumpVersion] Synced public/ui-version.json to ${uiPkg.version}`)
}

runStep(["run", "sync:version", "--workspace", "@embeddedcowork/tauri-app"], "tauri version sync")