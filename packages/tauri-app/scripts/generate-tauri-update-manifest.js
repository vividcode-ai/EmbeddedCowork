#!/usr/bin/env node

/**
 * generate-tauri-update-manifest.js
 *
 * Generates the tauri-update.json manifest for Tauri's built-in updater.
 * Called from CI after all platform builds are uploaded to a GitHub Release.
 *
 * Usage:
 *   node scripts/generate-tauri-update-manifest.js --tag v0.0.24 --version 0.0.24
 *
 * Environment variables:
 *   GITHUB_REPOSITORY - "owner/repo" (default: vividcode-ai/EmbeddedCowork)
 *   GITHUB_TOKEN      - GitHub API token for downloading signature files
 *   SIGNATURES_DIR    - Directory containing .sig files (default: ./release-tauri/signatures)
 */

const fs = require("fs")
const path = require("path")

// Parse CLI arguments
const args = {}
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i]
  if (arg.startsWith("--")) {
    const key = arg.slice(2)
    const val = process.argv[i + 1]
    if (val && !val.startsWith("--")) {
      args[key] = val
      i++
    } else {
      args[key] = true
    }
  }
}

const TAG = args.tag
const VERSION = args.version
if (!TAG || !VERSION) {
  console.error("Usage: node generate-tauri-update-manifest.js --tag v0.0.24 --version 0.0.24")
  process.exit(1)
}

const GITHUB_REPOSITORY = process.env.GITHUB_REPOSITORY || "vividcode-ai/EmbeddedCowork"
const SIGNATURES_DIR = process.env.SIGNATURES_DIR || path.join(__dirname, "..", "release-tauri", "signatures")
const RELEASE_NOTES_URL = `https://github.com/${GITHUB_REPOSITORY}/releases/tag/${TAG}`
const DOWNLOAD_BASE = `https://github.com/${GITHUB_REPOSITORY}/releases/download/${TAG}`

/**
 * Map Tauri artifact filenames to platform keys.
 * The key format is: {os}-{arch}
 */
const PLATFORM_MAP = {
  // macOS
  "EmbeddedCowork-Tauri-*-macos-x64.zip": "darwin-x86_64",
  "EmbeddedCowork-Tauri-*-macos-arm64.zip": "darwin-aarch64",
  // Windows
  "EmbeddedCowork-Tauri-*-windows-x64.zip": "windows-x86_64",
  "EmbeddedCowork-Tauri-*-windows-arm64.zip": "windows-arm64",
  // Linux
  "EmbeddedCowork-Tauri-*-linux-x64.AppImage": "linux-x86_64",
  "EmbeddedCowork-Tauri-*-linux-arm64.AppImage": "linux-arm64",
}

/**
 * Build a glob-like pattern matcher.
 * Converts "EmbeddedCowork-Tauri-*-macos-x64.zip" to a regex.
 */
function patternToRegex(pattern) {
  const escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&")
  const regexStr = escaped.replace(/\*/g, "(.+)")
  return new RegExp(`^${regexStr}$`)
}

/**
 * Read all .sig signature files from the signatures directory.
 * Returns a map of { rawFilename: signatureContent }.
 */
function readSignatureFiles(signaturesDir) {
  if (!fs.existsSync(signaturesDir)) {
    console.warn(`Warning: Signatures directory not found: ${signaturesDir}`)
    return {}
  }

  const entries = fs.readdirSync(signaturesDir)
  const signatures = {}

  for (const entry of entries) {
    if (!entry.endsWith(".sig")) continue
    const rawName = entry.replace(/\.sig$/, "")
    const sigPath = path.join(signaturesDir, entry)
    const content = fs.readFileSync(sigPath, "utf-8").trim()
    signatures[rawName] = content
  }

  return signatures
}

/**
 * Given a signature filename (without .sig), return the
 * corresponding platform key and download URL.
 */
function resolvePlatform(filename, version, downloadBase) {
  for (const [pattern, platformKey] of Object.entries(PLATFORM_MAP)) {
    const regex = patternToRegex(pattern.replace(/\*/g, version))
    if (regex.test(filename)) {
      // Reconstruct the original artifact filename using the version
      const artifactName = pattern.replace(/\*/g, version)
      return {
        platform: platformKey,
        url: `${downloadBase}/${artifactName}`,
      }
    }
  }
  return null
}

// Main
function main() {
  const signatures = readSignatureFiles(SIGNATURES_DIR)
  const platforms = {}
  const pubDate = new Date().toISOString()

  if (Object.keys(signatures).length === 0) {
    console.warn("Warning: No signature files found. Creating empty manifest.")
  }

  for (const [sigFilename, signature] of Object.entries(signatures)) {
    const resolved = resolvePlatform(sigFilename, VERSION, DOWNLOAD_BASE)
    if (!resolved) {
      console.warn(`Warning: Could not resolve platform for: ${sigFilename}`)
      continue
    }

    platforms[resolved.platform] = {
      signature,
      url: resolved.url,
    }
    console.log(`  ✓ ${resolved.platform}: ${resolved.url}`)
  }

  const manifest = {
    version: VERSION,
    notes: RELEASE_NOTES_URL,
    pub_date: pubDate,
    platforms,
  }

  const outputPath = path.join(__dirname, "..", "release-tauri", "tauri-update.json")
  fs.mkdirSync(path.dirname(outputPath), { recursive: true })
  fs.writeFileSync(outputPath, JSON.stringify(manifest, null, 2), "utf-8")

  console.log(`\n✅ Manifest generated: ${outputPath}`)
  console.log(`   Version: ${VERSION}`)
  console.log(`   Platforms: ${Object.keys(platforms).join(", ") || "(none)"}`)
}

main()
