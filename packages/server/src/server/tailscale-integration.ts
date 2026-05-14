import { spawn, type ChildProcess } from "child_process"
import { fetch } from "undici"
import path from "path"
import { fileURLToPath } from "url"
import fs from "fs"
import type { Logger } from "../logger"

export interface TailscaleStatus {
  ok: boolean
  connected: boolean
  tailscaleIPs: string[]
  hostname: string
  authNeeded: boolean
  authMethod: string
  loginURL?: string
  online: boolean
  error?: string
}

export interface TailscaleIntegrationOptions {
  sidecarPath: string
  stateDir: string
  controlURL?: string
  authKey?: string
  hostname?: string
  apiPort?: number
  logger: Logger
}

const HEALTH_CHECK_INTERVAL_MS = 10_000
const STARTUP_TIMEOUT_MS = 15_000
const PORT_TIMEOUT_MS = 10_000

export class TailscaleIntegration {
  private process: ChildProcess | null = null
  private apiBase = "http://127.0.0.1:0"
  private currentStatus: TailscaleStatus = {
    ok: false,
    connected: false,
    tailscaleIPs: [],
    hostname: "",
    authNeeded: false,
    authMethod: "none",
    online: false,
  }
  private healthTimer: ReturnType<typeof setInterval> | null = null
  private readonly logger: Logger
  private portResolved = false
  private resolvePort!: (port: number) => void
  private rejectPort!: (err: Error) => void
  private readonly portPromise: Promise<number>

  constructor(private readonly options: TailscaleIntegrationOptions) {
    this.logger = options.logger.child({ component: "tailscale" })
    if (options.apiPort && options.apiPort > 0) {
      this.portResolved = true
      this.apiBase = `http://127.0.0.1:${options.apiPort}`
      this.portPromise = Promise.resolve(options.apiPort)
    } else {
      this.portPromise = new Promise<number>((resolve, reject) => {
        this.resolvePort = resolve
        this.rejectPort = reject
      })
    }
  }

  async start(): Promise<void> {
    if (this.process) {
      this.logger.warn("sidecar already running")
      return
    }

    await this.ensureStateDir()

    const sidecarPath = this.options.sidecarPath
    if (!fs.existsSync(sidecarPath)) {
      this.logger.warn({ sidecarPath }, "tailscale sidecar binary not found, skipping")
      return
    }

    const args = [
      `--api-port=${this.options.apiPort ?? 0}`,
      `--state-dir=${this.options.stateDir}`,
      `--hostname=${this.options.hostname ?? "embeddedcowork"}`,
    ]

    if (this.options.controlURL) {
      args.push(`--control-url=${this.options.controlURL}`)
    }

    if (this.options.authKey) {
      args.push(`--auth-key=${this.options.authKey}`)
    }

    this.logger.info({ args, sidecarPath }, "starting tailscale sidecar")

    this.process = spawn(sidecarPath, args, {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    })

    this.process.stdout?.on("data", (data: Buffer) => {
      for (const line of data.toString().trim().split("\n")) {
        const trimmed = line.trim()
        const portMatch = trimmed.match(/^EC_SIDECAR_PORT=(\d+)$/)
        if (portMatch) {
          const port = parseInt(portMatch[1], 10)
          if (!this.portResolved) {
            this.portResolved = true
            this.apiBase = `http://127.0.0.1:${port}`
            this.options.apiPort = port
            this.resolvePort(port)
          }
        } else if (trimmed) {
          this.logger.info({ msg: trimmed }, "[ts-sidecar]")
        }
      }
    })

    this.process.stderr?.on("data", (data: Buffer) => {
      for (const line of data.toString().trim().split("\n")) {
        const msg = line.trim()
        if (msg) {
          this.logger.error({ msg }, "[ts-sidecar:err] %s", msg)
        }
      }
    })

    this.process.on("exit", (code, signal) => {
      this.logger.info({ code, signal }, "tailscale sidecar exited")
      this.process = null
      this.currentStatus = {
        ok: false,
        connected: false,
        tailscaleIPs: [],
        hostname: "",
        authNeeded: false,
        authMethod: "none",
        online: false,
      }
      if (!this.portResolved) {
        this.rejectPort(new Error(`sidecar exited (code=${code}) before reporting port`))
      }
    })

    this.process.on("error", (err) => {
      this.logger.error({ err }, "tailscale sidecar process error")
      this.process = null
      if (!this.portResolved) {
        this.rejectPort(err)
      }
    })

    await this.waitForPort()
    await this.waitForReady()
    this.startHealthChecks()
  }

  async stop(): Promise<void> {
    this.stopHealthChecks()

    if (this.process) {
      this.logger.info("stopping tailscale sidecar")
      this.process.kill("SIGTERM")

      await new Promise<void>((resolve) => {
        const timer = setTimeout(() => {
          this.logger.warn("force killing tailscale sidecar")
          this.process?.kill("SIGKILL")
          resolve()
        }, 5000)

        this.process?.on("exit", () => {
          clearTimeout(timer)
          resolve()
        })
      })

      this.process = null
    }
  }

  async getStatus(): Promise<TailscaleStatus> {
    if (!this.process) {
      return {
        ok: false,
        connected: false,
        tailscaleIPs: [],
        hostname: "",
        authNeeded: false,
        authMethod: "none",
        online: false,
        error: "sidecar not running",
      }
    }

    try {
      const response = await fetch(`${this.apiBase}/api/status`, {
        method: "GET",
        headers: { Accept: "application/json" },
        signal: AbortSignal.timeout(5000),
      })

      if (!response.ok) {
        throw new Error(`status api returned ${response.status}`)
      }

      const data = (await response.json()) as TailscaleStatus
      this.currentStatus = data
      return data
    } catch (error) {
      this.logger.warn({ err: error }, "failed to get tailscale status")
      return { ...this.currentStatus, ok: false, error: String(error) }
    }
  }

  async setAuthKey(authKey: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const response = await fetch(`${this.apiBase}/api/auth-key`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ authKey }),
        signal: AbortSignal.timeout(10000),
      })

      const data = (await response.json()) as { ok: boolean; error?: string }
      return data
    } catch (error) {
      return { ok: false, error: String(error) }
    }
  }

  async getLoginURL(): Promise<{ ok: boolean; url?: string; error?: string }> {
    try {
      const response = await fetch(`${this.apiBase}/api/login-url`, {
        method: "GET",
        headers: { Accept: "application/json" },
        signal: AbortSignal.timeout(5000),
      })

      const data = (await response.json()) as { ok: boolean; url?: string; error?: string }
      return data
    } catch (error) {
      return { ok: false, error: String(error) }
    }
  }

  async startForwarding(serverPort: number): Promise<void> {
    if (!this.process) {
      this.logger.warn("cannot start forwarding: sidecar not running")
      return
    }
    try {
      const res = await fetch(`${this.apiBase}/api/listen`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ localPort: serverPort }),
        signal: AbortSignal.timeout(5000),
      })
      const data = (await res.json()) as { ok: boolean; port?: number; tailscaleIPs?: string[]; error?: string }
      if (data.ok) {
        this.logger.info({ port: serverPort, tailscaleIPs: data.tailscaleIPs }, "Tailscale forwarding started")
      } else {
        this.logger.warn({ error: data.error }, "failed to start Tailscale forwarding")
      }
    } catch (error) {
      this.logger.error({ err: error }, "failed to start Tailscale forwarding")
    }
  }

  async stopForwarding(): Promise<void> {
    try {
      await fetch(`${this.apiBase}/api/listen`, { method: "DELETE", signal: AbortSignal.timeout(3000) })
    } catch {
      // ignore on stop
    }
  }

  isRunning(): boolean {
    return this.process !== null && this.currentStatus.ok
  }

  private async waitForPort(): Promise<void> {
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error("timed out waiting for sidecar port")), PORT_TIMEOUT_MS)
    )
    await Promise.race([this.portPromise, timeout])
  }

  private async waitForReady(): Promise<void> {
    const deadline = Date.now() + STARTUP_TIMEOUT_MS

    while (Date.now() < deadline) {
      try {
        const status = await this.getStatus()
        if (status.ok) {
          this.logger.info("tailscale sidecar ready")
          return
        }
      } catch {
        // not ready yet
      }

      await new Promise((resolve) => setTimeout(resolve, 500))
    }

    this.logger.warn("tailscale sidecar did not become ready within timeout")
  }

  private startHealthChecks(): void {
    this.healthTimer = setInterval(() => {
      this.getStatus().catch(() => {})
    }, HEALTH_CHECK_INTERVAL_MS)
  }

  private stopHealthChecks(): void {
    if (this.healthTimer) {
      clearInterval(this.healthTimer)
      this.healthTimer = null
    }
  }

  private async ensureStateDir(): Promise<void> {
    await fs.promises.mkdir(this.options.stateDir, { recursive: true })
  }
}

export function resolveTailscaleSidecarPath(): string {
  const currentDir = path.dirname(fileURLToPath(import.meta.url))
  const names = ["tailscale-sidecar", "tailscale-sidecar.exe"]

  const bases = [
    process.resourcesPath ?? "",
    path.join(currentDir, "..", "..", "..", "tailscale-sidecar", "bin"),
    path.join(currentDir, "..", "..", "..", "dist", "sidecar"),
    path.join(process.cwd(), "dist", "sidecar"),
    path.join(process.cwd(), "node_modules", ".bin"),
  ]

  for (const base of bases) {
    for (const name of names) {
      const p = path.join(base, name)
      if (fs.existsSync(p)) {
        return p
      }
    }
  }

  const fallback = path.join(bases[1]!, names[0]!)
  return fallback
}
