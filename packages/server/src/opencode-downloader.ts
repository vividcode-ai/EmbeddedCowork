import { createWriteStream, existsSync, mkdirSync, chmodSync, readFileSync, createReadStream, renameSync } from "fs"
import { stat, mkdir } from "fs/promises"
import path from "path"
import os from "os"
import { spawnSync } from "child_process"
import yauzl from "yauzl"
import { createGunzip } from "zlib"
import tar from "tar"
import { Logger } from "./logger"
import { BIN_DIR, BINARY_NAME } from "./opencode-paths"
import { downloadFileWithRetry } from "./download-utils"

const GITHUB_API = "https://api.github.com/repos/anomalyco/opencode/releases/latest"
const GITHUB_DL = "https://github.com/anomalyco/opencode/releases/latest/download"

interface DownloadTarget {
  filename: string
  archiveExt: ".zip" | ".tar.gz"
}

interface DownloadProgress {
  current: number
  total: number
}

export type DownloadStatus =
  | { type: "downloading"; progress: DownloadProgress }
  | { type: "extracting" }
  | { type: "verifying" }
  | { type: "completed"; binaryPath: string }
  | { type: "error"; message: string }

export type StatusCallback = (status: DownloadStatus) => void

export class OpencodeDownloader {
  private readonly logger?: Logger

  constructor(logger?: Logger) {
    this.logger = logger
  }

  getDownloadTarget(): DownloadTarget {
    const platform = process.platform
    const arch = process.arch

    let osName: string
    switch (platform) {
      case "win32":
        osName = "windows"
        break
      case "darwin":
        osName = "darwin"
        break
      case "linux":
        osName = "linux"
        break
      default:
        throw new Error(`Unsupported platform: ${platform}`)
    }

    let archName: string
    switch (arch) {
      case "x64":
        archName = "x64"
        break
      case "arm64":
        archName = "arm64"
        break
      default:
        throw new Error(`Unsupported architecture: ${arch}`)
    }

    let target = `${osName}-${archName}`

    if (arch === "x64" && !this.hasAvx2()) {
      target += "-baseline"
    }

    if (platform === "linux" && this.isMusl()) {
      target += "-musl"
    }

    const archiveExt: ".zip" | ".tar.gz" = platform === "linux" ? ".tar.gz" : ".zip"
    return { filename: `opencode-${target}${archiveExt}`, archiveExt }
  }

  private hasAvx2(): boolean {
    if (process.platform === "win32") {
      try {
        const ps = `(Add-Type -MemberDefinition "[DllImport(\"kernel32.dll\")] public static extern bool IsProcessorFeaturePresent(int ProcessorFeature);" -Name Kernel32 -Namespace Win32 -PassThru)::IsProcessorFeaturePresent(40)`
        const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", ps], { encoding: "utf8", timeout: 5000 })
        const out = result.stdout?.trim()?.toLowerCase()
        return out === "true" || out === "1"
      } catch {
        return true
      }
    }
    if (process.platform === "darwin") {
      try {
        const result = spawnSync("sysctl", ["-n", "hw.optional.avx2_0"], { encoding: "utf8" })
        return result.status === 0 && result.stdout.trim() === "1"
      } catch {
        return true
      }
    }
    if (process.platform === "linux") {
      try {
        const cpuinfo = readFileSync("/proc/cpuinfo", "utf8")
        return /avx2/i.test(cpuinfo)
      } catch {
        return true
      }
    }
    return true
  }

  private isMusl(): boolean {
    try {
      const result = spawnSync("ldd", ["--version"], { encoding: "utf8" })
      return result.stderr?.toLowerCase().includes("musl") ?? false
    } catch {
      return existsSync("/etc/alpine-release")
    }
  }

  private static versionCache: { version: string; expiry: number } | null = null

  async getLatestVersion(): Promise<string> {
    const now = Date.now()
    if (OpencodeDownloader.versionCache && now < OpencodeDownloader.versionCache.expiry) {
      return OpencodeDownloader.versionCache.version
    }
    const res = await fetch(GITHUB_API, {
      headers: { Accept: "application/json", "User-Agent": "embeddedcowork" },
      signal: AbortSignal.timeout(10000),
    })
    if (!res.ok) throw new Error(`Failed to fetch latest version: HTTP ${res.status}`)
    const data = (await res.json()) as { tag_name: string }
    const version = data.tag_name.replace(/^v/, "")
    OpencodeDownloader.versionCache = { version, expiry: now + 30000 }
    return version
  }

  private async extractZip(zipPath: string, outDir: string): Promise<void> {
    return new Promise((resolve, reject) => {
      yauzl.open(zipPath, { lazyEntries: true }, (err: Error | null, zipfile?: yauzl.ZipFile) => {
        if (err || !zipfile) return reject(err ?? new Error("Failed to open zip"))
        zipfile.readEntry()
        zipfile.on("entry", (entry: yauzl.Entry) => {
          if (entry.fileName === BINARY_NAME || entry.fileName.endsWith(`/${BINARY_NAME}`)) {
            zipfile.openReadStream(entry, (err2: Error | null, readStream?: NodeJS.ReadableStream) => {
              if (err2 || !readStream) return reject(err2 ?? new Error("Failed to open read stream"))
              const tmpPath = path.join(outDir, BINARY_NAME + ".tmp")
              const destPath = path.join(outDir, BINARY_NAME)
              const writer = createWriteStream(tmpPath)
              readStream.pipe(writer)
              writer.on("finish", () => {
                chmodSync(tmpPath, 0o755)
                renameSync(tmpPath, destPath)
                resolve()
              })
              writer.on("error", (e) => {
                try { import("fs/promises").then((fs) => fs.unlink(tmpPath)).catch(() => {}) } catch {}
                reject(e)
              })
            })
          } else {
            zipfile.readEntry()
          }
        })
        zipfile.on("error", reject)
        zipfile.on("end", () => reject(new Error("Binary not found in archive")))
      })
    })
  }

  private async extractTarGz(tarPath: string, outDir: string): Promise<void> {
    const extractPath = path.join(outDir, "extracted")
    await mkdir(extractPath, { recursive: true })

    await tar.extract({
      file: tarPath,
      cwd: extractPath,
      filter: (filePath: string) => filePath === BINARY_NAME || filePath.endsWith(`/${BINARY_NAME}`),
    } as tar.ExtractOptions)

    const binarySrc = path.join(extractPath, BINARY_NAME)
    const tmpPath = path.join(outDir, BINARY_NAME + ".tmp")
    const binaryDest = path.join(outDir, BINARY_NAME)

    await new Promise<void>((resolve, reject) => {
      const src = createReadStream(binarySrc)
      const dst = createWriteStream(tmpPath)
      src.pipe(dst)
      dst.on("finish", () => {
        chmodSync(tmpPath, 0o755)
        renameSync(tmpPath, binaryDest)
        resolve()
      })
      dst.on("error", (e) => {
        try { import("fs/promises").then((fs) => fs.unlink(tmpPath)).catch(() => {}) } catch {}
        reject(e)
      })
      src.on("error", (e) => {
        try { import("fs/promises").then((fs) => fs.unlink(tmpPath)).catch(() => {}) } catch {}
        reject(e)
      })
    })
  }

  private getInstalledPath(): string {
    return path.join(BIN_DIR, BINARY_NAME)
  }

  private async verifyBinary(binaryPath: string): Promise<string | null> {
    if (!existsSync(binaryPath)) return null
    try {
      const result = spawnSync(binaryPath, ["--version"], { encoding: "utf8", timeout: 5000 })
      if (result.status === 0) {
        return (result.stdout ?? result.stderr ?? "").trim()
      }
    } catch {}
    this.logger?.warn({ path: binaryPath }, "Binary exists but --version failed, proceeding anyway")
    return binaryPath
  }

  async ensureDownloaded(statusCb?: StatusCallback): Promise<string> {
    const target = this.getDownloadTarget()
    const binaryPath = this.getInstalledPath()

    const existingVersion = await this.verifyBinary(binaryPath)

    try {
      const latestVersion = await this.getLatestVersion()

      if (existingVersion && existingVersion.includes(latestVersion)) {
        this.logger?.info({ version: latestVersion, path: binaryPath }, "OpenCode binary is up to date")
        return binaryPath
      }
    } catch (err) {
      if (existingVersion) {
        this.logger?.warn({ err }, "Failed to check latest version, using existing binary")
        return binaryPath
      }
    }

    mkdirSync(BIN_DIR, { recursive: true })

    const tmpDir = path.join(os.tmpdir(), "embeddedcowork-dl")
    mkdirSync(tmpDir, { recursive: true })
    const archivePath = path.join(tmpDir, target.filename)
    const downloadUrl = `${GITHUB_DL}/${target.filename}`

    try {
      statusCb?.({ type: "downloading", progress: { current: 0, total: 0 } })

      await downloadFileWithRetry(
        downloadUrl,
        archivePath,
        {
          progressCb: (current, total) => {
            statusCb?.({ type: "downloading", progress: { current, total } })
          },
        },
      )

      statusCb?.({ type: "extracting" })

      if (target.archiveExt === ".zip") {
        await this.extractZip(archivePath, BIN_DIR)
      } else {
        await this.extractTarGz(archivePath, BIN_DIR)
      }

      statusCb?.({ type: "verifying" })

      const version = await this.verifyBinary(binaryPath)
      if (!version) {
        throw new Error("Downloaded binary failed verification")
      }

      this.logger?.info({ version, path: binaryPath }, "OpenCode binary downloaded and verified")
      statusCb?.({ type: "completed", binaryPath })
      return binaryPath
    } finally {
      try {
        await stat(archivePath)
        await import("fs/promises").then((fs) => fs.unlink(archivePath))
      } catch {}
    }
  }
}
