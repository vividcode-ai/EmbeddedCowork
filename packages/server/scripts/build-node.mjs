#!/usr/bin/env node
import fs from "fs"
import path from "path"
import { spawnSync } from "child_process"
import { fileURLToPath } from "url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const cliRoot = path.resolve(__dirname, "..")
const distDir = path.join(cliRoot, "dist")
const outputPath = path.join(distDir, "node", "node-entry.mjs")

function resolveEsbuildCommand() {
  const localBinName = process.platform === "win32" ? "esbuild.cmd" : "esbuild"
  const candidates = [
    path.join(cliRoot, "node_modules", ".bin", localBinName),
    path.join(cliRoot, "..", "..", "node_modules", ".bin", localBinName),
  ]
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate
    }
  }
  return "esbuild"
}

function main() {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true })
  const esbuild = resolveEsbuildCommand()
  const entry = path.join(cliRoot, "src", "node-entry.ts")

  const result = spawnSync(esbuild, [
    entry,
    "--bundle",
    "--platform=node",
    "--format=esm",
    "--target=node22",
    `--outfile=${outputPath}`,
    "--external:fastify",
    "--external:@fastify/*",
    "--external:undici",
    "--external:pino",
    "--external:zod",
    "--external:yaml",
    "--external:tar",
    "--external:yauzl",
    "--external:node-forge",
    "--external:openai",
    "--external:commander",
    "--external:fuzzysort",
    "--external:electron",
  ], {
    cwd: cliRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
  })

  if (result.status !== 0) {
    throw new Error(`esbuild exited with code ${result.status ?? 1}`)
  }

  console.log(`[build-node] built ${outputPath}`)
}

try {
  main()
} catch (error) {
  console.error("[build-node] failed:", error)
  process.exit(1)
}
