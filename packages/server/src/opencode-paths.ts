import { spawnSync } from "child_process"
import { existsSync } from "fs"
import path from "path"
import os from "os"
import { OpencodeDownloader } from "./opencode-downloader"
import type { Logger } from "./logger"

// ── Path constants ──────────────────────────────────────────

export const BIN_DIR = path.join(os.homedir(), ".embeddedcowork", "bin")
export const BINARY_NAME = process.platform === "win32" ? "opencode.exe" : "opencode"
export const INSTALLED_BINARY_PATH = path.join(BIN_DIR, BINARY_NAME)

// ── Shell helpers ───────────────────────────────────────────

function defaultShellPath(): string {
  const configured = process.env.SHELL?.trim()
  if (configured) return configured
  return process.platform === "darwin" ? "/bin/zsh" : "/bin/bash"
}

function shellEscape(input: string): string {
  if (!input) return "''"
  return `'${input.replace(/'/g, `'\\''`)}'`
}

function wrapCommandForShell(command: string, shellPath: string): string {
  const shellName = path.basename(shellPath).toLowerCase()
  if (shellName.includes("bash")) {
    return `if [ -f ~/.bashrc ]; then source ~/.bashrc >/dev/null 2>&1; fi; ${command}`
  }
  if (shellName.includes("zsh")) {
    return `if [ -f ~/.zshrc ]; then source ~/.zshrc >/dev/null 2>&1; fi; ${command}`
  }
  return command
}

function buildShellArgs(shellPath: string, command: string): string[] {
  const shellName = path.basename(shellPath).toLowerCase()
  if (shellName.includes("zsh")) return ["-l", "-i", "-c", command]
  return ["-l", "-c", command]
}

// ── Binary lookup ───────────────────────────────────────────

export function resolveBinaryPathFromUserShell(identifier: string): string | null {
  if (process.platform === "win32") return null
  const shellPath = defaultShellPath()
  const lookupCommand = wrapCommandForShell(`command -v ${shellEscape(identifier)}`, shellPath)
  const result = spawnSync(shellPath, buildShellArgs(shellPath, lookupCommand), {
    encoding: "utf8",
    env: {
      ...process.env,
      npm_config_prefix: undefined,
      NPM_CONFIG_PREFIX: undefined,
    },
  })
  if (result.status !== 0) return null
  const resolved = String(result.stdout ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0)
  return resolved ?? null
}

/**
 * Resolve an opencode binary identifier to an absolute path.
 * Order: absolute/relative → which/where → shell command -v → installed path → raw identifier
 */
export function resolveBinary(identifier: string): string {
  if (!identifier) return identifier
  if (path.isAbsolute(identifier) || identifier.includes("/") || identifier.includes("\\") || identifier.startsWith(".")) {
    return identifier
  }
  const locator = process.platform === "win32" ? "where" : "which"
  try {
    const result = spawnSync(locator, [identifier], { encoding: "utf8" })
    if (result.status === 0 && result.stdout) {
      const candidates = result.stdout
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line.length > 0)
        .filter((line) => !/^INFO:/i.test(line))
      if (candidates.length > 0) {
        return candidates[0]
      }
    }
  } catch {}
  const shellResolved = resolveBinaryPathFromUserShell(identifier)
  if (shellResolved) return shellResolved
  if (existsSync(INSTALLED_BINARY_PATH)) return INSTALLED_BINARY_PATH
  return identifier
}

// ── Download status tracking ────────────────────────────────

export type DownloadPhase = "idle" | "downloading" | "extracting" | "verifying" | "completed" | "error"

let downloadPhase: DownloadPhase = "idle"
let downloadProgress = { current: 0, total: 0 }
let downloadError: string | undefined

function setDownloadPhase(phase: DownloadPhase) {
  downloadPhase = phase
  if (phase === "idle" || phase === "completed" || phase === "error") {
    downloadProgress = { current: 0, total: 0 }
  }
}

export function getDownloadProgress() {
  return { ...downloadProgress }
}

export function getDownloadPhase(): DownloadPhase {
  return downloadPhase
}

export function getDownloadError(): string | undefined {
  return downloadError
}

/**
 * Check whether opencode is available on this system.
 * Uses the same lookup order as resolveBinary("opencode").
 * Returns additional download progress info when not available.
 */
export function getOpencodeStatus(): {
  available: boolean
  status?: DownloadPhase
  progress?: { current: number; total: number }
  error?: string
} {
  const resolved = resolveBinary("opencode")
  const available = resolved !== "opencode" && existsSync(resolved)
  if (available) return { available: true }
  return {
    available: false,
    status: downloadPhase,
    progress: downloadPhase === "downloading" ? { ...downloadProgress } : undefined,
    error: downloadError,
  }
}

export function isBinaryAvailable(): boolean {
  const resolved = resolveBinary("opencode")
  return resolved !== "opencode" && existsSync(resolved)
}

// ── Download trigger (global singleton) ─────────────────────

let downloadPromise: Promise<void> | null = null

export function getDownloadPromise(): Promise<void> | null {
  return downloadPromise
}

export function triggerBinaryDownload(logger: Logger): Promise<void> {
  if (downloadPromise) return downloadPromise
  setDownloadPhase("downloading")
  downloadError = undefined
  const downloader = new OpencodeDownloader(logger)
  downloadPromise = downloader
    .ensureDownloaded((status) => {
      if (status.type === "downloading") {
        downloadPhase = "downloading"
        downloadProgress = status.progress
      } else if (status.type === "extracting") {
        setDownloadPhase("extracting")
      } else if (status.type === "verifying") {
        setDownloadPhase("verifying")
      } else if (status.type === "completed") {
        setDownloadPhase("completed")
      }
    })
    .then(() => {
      logger.info("OpenCode auto-download completed")
      setDownloadPhase("completed")
      downloadPromise = null
    })
    .catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err)
      logger.error({ err }, "OpenCode auto-download failed")
      downloadPhase = "error"
      downloadError = message
    })
  return downloadPromise
}
