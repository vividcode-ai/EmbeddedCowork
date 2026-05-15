import { spawn, spawnSync, type ChildProcess } from "child_process"
import { app, utilityProcess, type UtilityProcess } from "electron"
import { createRequire } from "module"
import { EventEmitter } from "events"
import { existsSync, mkdirSync, readFileSync, readdirSync, unlinkSync } from "fs"
import os from "os"
import path from "path"
import { fileURLToPath } from "url"
import { parse as parseYaml } from "yaml"
import { buildUserShellCommand, getUserShellEnv, supportsUserShell } from "./user-shell"

const nodeRequire = createRequire(import.meta.url)
const mainFilename = fileURLToPath(import.meta.url)
const mainDirname = path.dirname(mainFilename)

const BOOTSTRAP_TOKEN_PREFIX = "EMBEDDEDCOWORK_BOOTSTRAP_TOKEN:"
const SESSION_COOKIE_NAME_PREFIX = "embeddedcowork_session"

type CliState = "starting" | "ready" | "error" | "stopped"
type ListeningMode = "local" | "all"

export interface CliStatus {
  state: CliState
  pid?: number
  port?: number
  url?: string
  error?: string
}

export interface CliLogEntry {
  stream: "stdout" | "stderr"
  message: string
}

interface StartOptions {
  dev: boolean
}

interface CliEntryResolution {
  entry: string
  runner: "node" | "tsx" | "standalone" | "system"
  runnerPath?: string
}

type ManagedChild = ChildProcess | UtilityProcess
type ChildLaunchMode = "spawn" | "utility"

const DEFAULT_CONFIG_PATH = "~/.config/embeddedcowork/config.json"

function isYamlPath(filePath: string): boolean {
  const lower = filePath.toLowerCase()
  return lower.endsWith(".yaml") || lower.endsWith(".yml")
}

function isJsonPath(filePath: string): boolean {
  return filePath.toLowerCase().endsWith(".json")
}

function resolveConfigPaths(raw?: string): { configYamlPath: string; legacyJsonPath: string } {
  const target = raw && raw.trim().length > 0 ? raw.trim() : DEFAULT_CONFIG_PATH
  const resolved = resolveConfigPath(target)

  if (isYamlPath(resolved)) {
    const baseDir = path.dirname(resolved)
    return { configYamlPath: resolved, legacyJsonPath: path.join(baseDir, "config.json") }
  }

  if (isJsonPath(resolved)) {
    const baseDir = path.dirname(resolved)
    return { configYamlPath: path.join(baseDir, "config.yaml"), legacyJsonPath: resolved }
  }

  // Treat as directory.
  return {
    configYamlPath: path.join(resolved, "config.yaml"),
    legacyJsonPath: path.join(resolved, "config.json"),
  }
}

function resolveConfigPath(configPath?: string): string {
  const target = configPath && configPath.trim().length > 0 ? configPath : DEFAULT_CONFIG_PATH
  if (target.startsWith("~/")) {
    return path.join(os.homedir(), target.slice(2))
  }
  return path.resolve(target)
}

function resolveHostForMode(mode: ListeningMode): string {
  return mode === "local" ? "127.0.0.1" : "0.0.0.0"
}

function readListeningModeFromConfig(): ListeningMode {
  try {
    const { configYamlPath, legacyJsonPath } = resolveConfigPaths(process.env.CLI_CONFIG)

    let parsed: any = null
    if (existsSync(configYamlPath)) {
      const content = readFileSync(configYamlPath, "utf-8")
      parsed = parseYaml(content)
    } else if (existsSync(legacyJsonPath)) {
      const content = readFileSync(legacyJsonPath, "utf-8")
      parsed = JSON.parse(content)
    } else {
      return "local"
    }

    const mode = parsed?.server?.listeningMode ?? parsed?.preferences?.listeningMode
    if (mode === "local" || mode === "all") {
      return mode
    }
  } catch (error) {
    console.warn("[cli] failed to read listening mode from config", error)
  }
  return "local"
}

export declare interface CliProcessManager {
  on(event: "status", listener: (status: CliStatus) => void): this
  on(event: "ready", listener: (status: CliStatus) => void): this
  on(event: "bootstrapToken", listener: (token: string) => void): this
  on(event: "log", listener: (entry: CliLogEntry) => void): this
  on(event: "exit", listener: (status: CliStatus) => void): this
  on(event: "error", listener: (error: Error) => void): this
}

export class CliProcessManager extends EventEmitter {
  private child?: ManagedChild
  private childLaunchMode: ChildLaunchMode = "spawn"
  private status: CliStatus = { state: "stopped" }
  private stdoutBuffer = ""
  private stderrBuffer = ""
  private stderrFullBuffer = ""
  private bootstrapToken: string | null = null
  private authCookieName = `${SESSION_COOKIE_NAME_PREFIX}_${process.pid}_${Date.now()}`
  private requestedStop = false

  async start(options: StartOptions): Promise<CliStatus> {
    if (this.child) {
      await this.stop()
    }

    this.stdoutBuffer = ""
    this.stderrBuffer = ""
    this.stderrFullBuffer = ""
    this.bootstrapToken = null
    this.authCookieName = `${SESSION_COOKIE_NAME_PREFIX}_${process.pid}_${Date.now()}`
    this.requestedStop = false
    this.updateStatus({ state: "starting", port: undefined, pid: undefined, url: undefined, error: undefined })

    const listeningMode = this.resolveListeningMode()
    const host = resolveHostForMode(listeningMode)
    const args = this.buildCliArgs(options, host)

    if (options.dev) {
      const tsxPath = this.resolveTsx()
      if (!tsxPath) {
        throw new Error("tsx is required to run the CLI in development mode. Please install dependencies.")
      }
      const devEntry = this.resolveDevEntry()
      return this.launchEntry({ entry: devEntry, runner: "tsx", runnerPath: tsxPath }, args, options)
    }

    const systemEntry = this.resolveSystemEntry()
    if (systemEntry) {
      try {
        return await this.launchEntry({ entry: systemEntry, runner: "system" }, args, options)
      } catch (err) {
        this.child = undefined
        this.requestedStop = false
        this.updateStatus({ state: "starting", port: undefined, pid: undefined, url: undefined, error: undefined })
        console.warn(`[cli] system entry failed, falling back to bundled server: ${(err as Error).message}`)
      }
    }

    return this.launchEntry({ entry: this.resolveStandaloneProdEntry(), runner: "standalone" }, args, options)
  }

  private launchEntry(cliEntry: CliEntryResolution, args: string[], options: StartOptions): Promise<CliStatus> {
    const host = resolveHostForMode(this.resolveListeningMode())

    let child: ManagedChild

    if (this.shouldUsePackagedShellSupervisor(options, cliEntry)) {
      const supervisorPath = this.resolveCliSupervisorPath()
      const shellEnv = supportsUserShell() ? getUserShellEnv() : { ...process.env }
      const shellTarget = cliEntry.runner === "standalone" || cliEntry.runner === "system" ? this.buildExecutableCommand(cliEntry.entry, args) : this.buildCommand(cliEntry, args)
      const shellCommand = buildUserShellCommand(`exec ${shellTarget}`)
      const supervisorPayload = JSON.stringify({
        command: shellCommand.command,
        args: shellCommand.args,
        cwd: process.cwd(),
      })

      console.info(
        `[cli] launching EmbeddedCowork CLI (${options.dev ? "dev" : "prod"}) via utility supervisor using ${cliEntry.runner} at ${cliEntry.entry} (host=${host})`,
      )
      console.info(`[cli] utility supervisor: ${supervisorPath}`)
      console.info(`[cli] shell command: ${shellCommand.command} ${shellCommand.args.join(" ")}`)

      child = utilityProcess.fork(supervisorPath, [supervisorPayload], {
        env: cliEntry.runner === "standalone" || cliEntry.runner === "system" ? shellEnv : { ...shellEnv, ELECTRON_RUN_AS_NODE: "1" },
        stdio: "pipe",
        serviceName: "EmbeddedCowork CLI Supervisor",
      })
      this.childLaunchMode = "utility"
    } else {
      console.info(
        `[cli] launching EmbeddedCowork CLI (${options.dev ? "dev" : "prod"}) using ${cliEntry.runner} at ${cliEntry.entry} (host=${host})`,
      )

      const env = supportsUserShell() ? getUserShellEnv() : { ...process.env }
      if (cliEntry.runner !== "standalone" && cliEntry.runner !== "system") {
        env.ELECTRON_RUN_AS_NODE = "1"
      }

      const spawnDetails = supportsUserShell()
        ? buildUserShellCommand(
            `${cliEntry.runner === "standalone" || cliEntry.runner === "system" ? "" : "ELECTRON_RUN_AS_NODE=1 "}exec ${
              cliEntry.runner === "standalone" || cliEntry.runner === "system" ? this.buildExecutableCommand(cliEntry.entry, args) : this.buildCommand(cliEntry, args)
            }`,
          )
        : this.buildDirectSpawn(cliEntry, args)

      const detached = process.platform !== "win32"
      child = spawn(spawnDetails.command, spawnDetails.args, {
        cwd: process.cwd(),
        stdio: ["ignore", "pipe", "pipe"],
        env,
        shell: false,
        detached,
      })

      console.info(`[cli] spawn command: ${spawnDetails.command} ${spawnDetails.args.join(" ")}`)
      this.childLaunchMode = "spawn"
    }

    if (this.childLaunchMode === "spawn" && !child.pid) {
      console.error("[cli] spawn failed: no pid")
    }

    this.child = child
    this.updateStatus({ pid: child.pid ?? undefined })

    child.stdout?.on("data", (data: Buffer) => {
      this.handleStream(data.toString(), "stdout")
    })

    child.stderr?.on("data", (data: Buffer) => {
      this.handleStream(data.toString(), "stderr")
    })

    if (this.childLaunchMode === "utility") {
      const utilityChild = child as UtilityProcess

      utilityChild.on("error", (error) => {
        const message = this.describeUtilityProcessError(error)
        console.error("[cli] utility supervisor failed:", error)
        this.updateStatus({ state: "error", error: message })
        this.emit("error", new Error(message))
      })

      utilityChild.on("exit", (code) => {
        const failed = this.status.state !== "ready"
        const diagStderr = this.stderrFullBuffer + this.stderrBuffer
        const diagSuffix = diagStderr ? `\n\n--- server stderr ---\n${diagStderr}` : ""
        const error = failed ? this.status.error ?? `CLI exited with code ${code ?? 0}${diagSuffix}` : undefined
        console.info(`[cli] exit (code=${code ?? ""})${error ? ` error=${error}` : ""}`)
        this.updateStatus({ state: failed ? "error" : "stopped", error })
        if (failed && error) {
          this.emit("error", new Error(error))
        }
        this.emit("exit", this.status)
        this.child = undefined
      })
    } else {
      const spawnedChild = child as ChildProcess

      spawnedChild.on("error", (error) => {
        console.error("[cli] failed to start CLI:", error)
        this.updateStatus({ state: "error", error: error.message })
        this.emit("error", error)
      })

      spawnedChild.on("exit", (code, signal) => {
        const failed = this.status.state !== "ready"
        const diagStderr = this.stderrFullBuffer + this.stderrBuffer
        const diagSuffix = diagStderr ? `\n\n--- server stderr ---\n${diagStderr}` : ""
        const error = failed ? this.status.error ?? `CLI exited with code ${code ?? 0}${signal ? ` (${signal})` : ""}${diagSuffix}` : undefined
        console.info(`[cli] exit (code=${code}, signal=${signal || ""})${error ? ` error=${error}` : ""}`)
        this.updateStatus({ state: failed ? "error" : "stopped", error })
        if (failed && error) {
          this.emit("error", new Error(error))
        }
        this.emit("exit", this.status)
        this.child = undefined
      })
    }

    return new Promise<CliStatus>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.handleTimeout()
        reject(new Error("CLI startup timeout"))
      }, 60000)

      this.once("ready", (status) => {
        clearTimeout(timeout)
        this.cleanupStaleTempFile()
        this.checkAndDownloadUpdate().catch(() => {})
        resolve(status)
      })

      this.once("error", (error) => {
        clearTimeout(timeout)
        reject(error)
      })
    })
  }

  async stop(): Promise<void> {
    const child = this.child
    if (!child) {
      this.updateStatus({ state: "stopped" })
      return
    }

    if (this.childLaunchMode === "utility") {
      return this.stopUtilityChild(child as UtilityProcess)
    }

    const spawnedChild = child as ChildProcess

    this.requestedStop = true

    const pid = spawnedChild.pid
    if (!pid) {
      this.child = undefined
      this.updateStatus({ state: "stopped" })
      return
    }

    const isAlreadyExited = () => spawnedChild.exitCode !== null || spawnedChild.signalCode !== null

    const tryKillPosixGroup = (signal: NodeJS.Signals) => {
      try {
        // Negative PID targets the process group (POSIX).
        process.kill(-pid, signal)
        return true
      } catch (error) {
        const err = error as NodeJS.ErrnoException
        if (err?.code === "ESRCH") {
          return true
        }
        return false
      }
    }

    const tryKillSinglePid = (signal: NodeJS.Signals) => {
      try {
        process.kill(pid, signal)
        return true
      } catch (error) {
        const err = error as NodeJS.ErrnoException
        if (err?.code === "ESRCH") {
          return true
        }
        return false
      }
    }

    const tryTaskkill = (force: boolean) => {
      const args = ["/PID", String(pid), "/T"]
      if (force) {
        args.push("/F")
      }

      try {
        const result = spawnSync("taskkill", args, { encoding: "utf8" })
        const exitCode = result.status
        if (exitCode === 0) {
          return true
        }

        // If the PID is already gone, treat it as success.
        const stderr = (result.stderr ?? "").toString().toLowerCase()
        const stdout = (result.stdout ?? "").toString().toLowerCase()
        const combined = `${stdout}\n${stderr}`
        if (combined.includes("not found") || combined.includes("no running instance")) {
          return true
        }
        return false
      } catch {
        return false
      }
    }

    const sendStopSignal = (signal: NodeJS.Signals) => {
      if (process.platform === "win32") {
        tryTaskkill(signal === "SIGKILL")
        return
      }

      // Prefer process-group signaling so wrapper launchers (shell/tsx) don't outlive Electron.
      const groupOk = tryKillPosixGroup(signal)
      if (!groupOk) {
        tryKillSinglePid(signal)
      }
    }

    return new Promise((resolve) => {
      const killTimeout = setTimeout(() => {
        console.warn(
          `[cli] stop timed out after 30000ms; sending SIGKILL (pid=${child.pid ?? "unknown"})`,
        )
        sendStopSignal("SIGKILL")
      }, 30000)

      spawnedChild.on("exit", () => {
        clearTimeout(killTimeout)
        this.child = undefined
        console.info("[cli] CLI process exited")
        this.updateStatus({ state: "stopped" })
        resolve()
      })

      if (isAlreadyExited()) {
        clearTimeout(killTimeout)
        this.child = undefined
        this.updateStatus({ state: "stopped" })
        resolve()
        return
      }

      sendStopSignal("SIGTERM")
    })
  }

  private stopUtilityChild(child: UtilityProcess): Promise<void> {
    this.requestedStop = true

    const pid = child.pid
    if (!pid) {
      this.child = undefined
      this.updateStatus({ state: "stopped" })
      return Promise.resolve()
    }

    return new Promise((resolve) => {
      const killTimeout = setTimeout(() => {
        console.warn(`[cli] stop timed out after 30000ms; sending SIGKILL (pid=${pid})`)
        try {
          process.kill(pid, "SIGKILL")
        } catch {
          // no-op
        }
      }, 30000)

      child.once("exit", () => {
        clearTimeout(killTimeout)
        this.child = undefined
        console.info("[cli] CLI process exited")
        this.updateStatus({ state: "stopped" })
        resolve()
      })

      if (child.pid === undefined) {
        clearTimeout(killTimeout)
        this.child = undefined
        this.updateStatus({ state: "stopped" })
        resolve()
        return
      }

      child.kill()
    })
  }

  getStatus(): CliStatus {
    return { ...this.status }
  }

  getAuthCookieName(): string {
    return this.authCookieName
  }

  private resolveListeningMode(): ListeningMode {
    return readListeningModeFromConfig()
  }

  private handleTimeout() {
    if (this.child) {
      const pid = this.child.pid
      if (this.childLaunchMode === "utility") {
        if (pid) {
          try {
            process.kill(pid, "SIGKILL")
          } catch {
            // no-op
          }
        }
      } else if (pid && process.platform !== "win32") {
        try {
          process.kill(-pid, "SIGKILL")
        } catch {
          ;(this.child as ChildProcess).kill("SIGKILL")
        }
      } else {
        ;(this.child as ChildProcess).kill("SIGKILL")
      }
      this.child = undefined
    }
    this.updateStatus({ state: "error", error: "CLI did not start in time" })
    this.emit("error", new Error("CLI did not start in time"))
  }

  private handleStream(chunk: string, stream: "stdout" | "stderr") {
    if (stream === "stdout") {
      this.stdoutBuffer += chunk
      this.processBuffer("stdout")
    } else {
      this.stderrBuffer += chunk
      this.processBuffer("stderr")
    }
  }

  private processBuffer(stream: "stdout" | "stderr") {
    const buffer = stream === "stdout" ? this.stdoutBuffer : this.stderrBuffer
    const lines = buffer.split("\n")
    const trailing = lines.pop() ?? ""

    if (stream === "stdout") {
      this.stdoutBuffer = trailing
    } else {
      this.stderrBuffer = trailing
      if (lines.length > 0) {
        this.stderrFullBuffer += lines.join("\n") + "\n"
        if (this.stderrFullBuffer.length > 10000) {
          this.stderrFullBuffer = this.stderrFullBuffer.slice(-10000)
        }
      }
    }

    for (const line of lines) {
      const trimmed = line.trim()
      if (!trimmed) continue

      if (trimmed.startsWith(BOOTSTRAP_TOKEN_PREFIX)) {
        const token = trimmed.slice(BOOTSTRAP_TOKEN_PREFIX.length).trim()
        if (token && !this.bootstrapToken) {
          this.bootstrapToken = token
          this.emit("bootstrapToken", token)
        }
        continue
      }

      console.info(`[cli][${stream}] ${trimmed}`)
      this.emit("log", { stream, message: trimmed })

      const localUrl = this.extractLocalUrl(trimmed)
      if (localUrl && this.status.state === "starting") {
        let port: number | undefined
        try {
          port = Number(new URL(localUrl).port) || undefined
        } catch {
          port = undefined
        }
        console.info(`[cli] ready on ${localUrl}`)
        this.updateStatus({ state: "ready", port, url: localUrl })
        this.emit("ready", this.status)
      }
    }
  }

  private extractLocalUrl(line: string): string | null {
    const match = line.match(/^Local\s+Connection\s+URL\s*:\s*(https?:\/\/\S+)\s*$/i)
    if (!match) {
      return null
    }
    return match[1] ?? null
  }

  private updateStatus(patch: Partial<CliStatus>) {
    this.status = { ...this.status, ...patch }
    this.emit("status", this.status)
  }

  private buildCliArgs(options: StartOptions, host: string): string[] {
    const args = ["--host", host, "--generate-token", "--auth-cookie-name", this.authCookieName, "--unrestricted-root"]

    if (options.dev) {
      // Dev: run plain HTTP + Vite dev server proxy.
      args.push("--https", "false", "--http", "true")
      // Avoid collisions with an already-running server (and dual-stack ::/0.0.0.0 quirks)
      // by forcing an ephemeral port in dev.
      args.push("--http-port", "0")
    } else {
      // Prod desktop: always keep loopback HTTP enabled.
      args.push("--https", "true", "--http", "true")
    }

    if (options.dev) {
      const devServer = process.env.VITE_DEV_SERVER_URL || process.env.ELECTRON_RENDERER_URL || "http://localhost:3000"
      const rawLogLevel = (process.env.CLI_LOG_LEVEL ?? "info").trim()
      const logLevel = rawLogLevel.length > 0 ? rawLogLevel.toLowerCase() : "info"
      args.push("--ui-dev-server", devServer, "--log-level", logLevel)
    }

    return args
  }

  private buildCommand(cliEntry: CliEntryResolution, args: string[]): string {
    if (cliEntry.runner === "standalone" || cliEntry.runner === "system") {
      return this.buildExecutableCommand(cliEntry.entry, args)
    }

    const parts = [JSON.stringify(process.execPath)]
    if (cliEntry.runner === "tsx" && cliEntry.runnerPath) {
      parts.push(JSON.stringify(cliEntry.runnerPath))
    }
    parts.push(JSON.stringify(cliEntry.entry))
    args.forEach((arg) => parts.push(JSON.stringify(arg)))
    return parts.join(" ")
  }

  private buildExecutableCommand(command: string, args: string[]): string {
    return [JSON.stringify(command), ...args.map((arg) => JSON.stringify(arg))].join(" ")
  }

  private buildDirectSpawn(cliEntry: CliEntryResolution, args: string[]) {
    if (cliEntry.runner === "standalone" || cliEntry.runner === "system") {
      return { command: cliEntry.entry, args }
    }

    if (cliEntry.runner === "tsx") {
      return { command: process.execPath, args: [cliEntry.runnerPath!, cliEntry.entry, ...args] }
    }

    return { command: process.execPath, args: [cliEntry.entry, ...args] }
  }

 
  private resolveTsx(): string | null {
    const candidates: Array<string | (() => string)> = [
      () => nodeRequire.resolve("tsx/cli"),
      () => nodeRequire.resolve("tsx/dist/cli.mjs"),
      () => nodeRequire.resolve("tsx/dist/cli.cjs"),
      path.resolve(process.cwd(), "node_modules", "tsx", "dist", "cli.mjs"),
      path.resolve(process.cwd(), "node_modules", "tsx", "dist", "cli.cjs"),
      path.resolve(process.cwd(), "..", "node_modules", "tsx", "dist", "cli.mjs"),
      path.resolve(process.cwd(), "..", "node_modules", "tsx", "dist", "cli.cjs"),
      path.resolve(process.cwd(), "..", "..", "node_modules", "tsx", "dist", "cli.mjs"),
      path.resolve(process.cwd(), "..", "..", "node_modules", "tsx", "dist", "cli.cjs"),
      path.resolve(app.getAppPath(), "..", "node_modules", "tsx", "dist", "cli.mjs"),
      path.resolve(app.getAppPath(), "..", "node_modules", "tsx", "dist", "cli.cjs"),
    ]
 
    for (const candidate of candidates) {
      try {
        const resolved = typeof candidate === "function" ? candidate() : candidate
        if (resolved && existsSync(resolved)) {
          return resolved
        }
      } catch {
        continue
      }
    }
 
    return null
  }
 
  private resolveDevEntry(): string {
    const entry = path.resolve(process.cwd(), "..", "server", "src", "index.ts")
    if (!existsSync(entry)) {
      throw new Error(`Dev CLI entry not found at ${entry}. Run npm run dev:electron from the repository root after installing dependencies.`)
    }
    return entry
  }
 
  private resolveStandaloneProdEntry(): string {
    const executableName = process.platform === "win32" ? "embeddedcowork-server.exe" : "embeddedcowork-server"
    const candidates = [
      path.join(process.resourcesPath, "server", "dist", executableName),
      path.join(mainDirname, "../resources/server/dist", executableName),
      path.resolve(process.cwd(), "..", "server", "dist", executableName),
    ]

    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        return candidate
      }
    }

    throw new Error(`Unable to locate standalone EmbeddedCowork server executable (${executableName}). Run npm run build:standalone --workspace @vividcodeai/embeddedcowork.`)
  }

  /**
   * 三步检测链：
   *   1. which/where → "embeddedcowork-server" on PATH (npm 全局安装)
   *   2. ~/.embeddedcowork/bin/embeddedcowork-server (自动更新的 binary 名)
   *   3. 未找到 → return null
   */
  private resolveSystemEntry(): string | null {
    const locator = process.platform === "win32" ? "where" : "which"
    try {
      const result = spawnSync(locator, ["embeddedcowork-server"], { encoding: "utf8" })
      if (result.status === 0 && result.stdout) {
        const candidates = result.stdout
          .split(/\r?\n/)
          .map((line: string) => line.trim())
          .filter((line: string) => line.length > 0)
          .filter((line: string) => !/^INFO:/i.test(line))
          .filter((line: string) => process.platform !== "win32" || /\.(exe|cmd|bat)$/i.test(line))
        if (candidates.length > 0 && existsSync(candidates[0])) {
          return candidates[0]
        }
      }
    } catch {}

    const serverDir = path.join(os.homedir(), ".embeddedcowork", "bin", "server")
    const pkgPath = path.join(serverDir, "package.json")
    if (existsSync(pkgPath)) {
      try {
        const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"))
        if (pkg.version) {
          const ext = process.platform === "win32" ? ".exe" : ""
          const versionedPath = path.join(serverDir, `embeddedcowork-server-${pkg.version}${ext}`)
          if (existsSync(versionedPath)) return versionedPath
        }
      } catch {
        // package.json 损坏，忽略
      }
    }

    return null
  }

  private shouldUsePackagedShellSupervisor(options: StartOptions, cliEntry: CliEntryResolution): boolean {
    return !options.dev && app.isPackaged && process.platform === "darwin" && cliEntry.runner !== "standalone" && cliEntry.runner !== "system"
  }

  private resolveCliSupervisorPath(): string {
    const candidates = [
      path.join(process.resourcesPath, "cli-supervisor.cjs"),
      path.join(mainDirname, "../resources/cli-supervisor.cjs"),
    ]

    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        return candidate
      }
    }

    throw new Error("Unable to locate EmbeddedCowork CLI supervisor script.")
  }

  private describeUtilityProcessError(error: unknown): string {
    if (error instanceof Error && error.message) {
      return error.message
    }

    if (error && typeof error === "object") {
      const typed = error as { type?: unknown; location?: unknown }
      if (typeof typed.type === "string") {
        return typeof typed.location === "string" ? `${typed.type} at ${typed.location}` : typed.type
      }
    }

    return String(error)
  }

  private resolveDownloadWorkerPath(): string {
    const candidates = [
      path.join(process.resourcesPath, "download-worker.cjs"),
      path.join(mainDirname, "../../electron/resources/download-worker.cjs"),
    ]

    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        return candidate
      }
    }

    throw new Error("Unable to locate download worker script.")
  }

  private cleanupStaleTempFile(): void {
    const serverDir = path.join(os.homedir(), ".embeddedcowork", "bin", "server")
    if (!existsSync(serverDir)) return
    for (const name of readdirSync(serverDir)) {
      if (name.endsWith(".tmp")) {
        unlinkSync(path.join(serverDir, name))
      }
    }
  }

  private async checkAndDownloadUpdate(): Promise<void> {
    try {
      const currentVersion = app.getVersion()

      const response = await fetch(
        "https://api.github.com/repos/vividcode-ai/EmbeddedCowork/releases/latest",
        { headers: { Accept: "application/vnd.github.v3+json", "User-Agent": "EmbeddedCowork" } },
      )
      if (!response.ok) return
      const data = (await response.json()) as { tag_name: string }
      const latestTag = data.tag_name
      const latestVersion = latestTag.startsWith("v") ? latestTag.slice(1) : latestTag

      if (this.compareVersions(latestVersion, currentVersion) <= 0) return

      const platform = this.getPlatformKey()
      if (!platform) return
      const ext = process.platform === "win32" ? ".exe" : ""
      const assetName = `embeddedcowork-server-${latestVersion}-${platform}${ext}`
      const downloadUrl = `https://github.com/vividcode-ai/EmbeddedCowork/releases/download/${latestTag}/${assetName}`

      const serverDir = path.join(os.homedir(), ".embeddedcowork", "bin", "server")

      mkdirSync(serverDir, { recursive: true })

      const workerPath = this.resolveDownloadWorkerPath()
      const child = spawn(process.execPath, [workerPath, downloadUrl, serverDir, latestVersion], {
        detached: true,
        stdio: "ignore",
        env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" },
        windowsHide: true,
      })
      child.unref()

      console.info(`[cli] spawning background update worker for v${latestVersion}`)
    } catch {
      // 静默失败，不影响正在运行的 server
    }
  }

  private compareVersions(a: string, b: string): number {
    const pa = a.split(".").map(Number)
    const pb = b.split(".").map(Number)
    for (let i = 0; i < 3; i++) {
      const diff = (pa[i] ?? 0) - (pb[i] ?? 0)
      if (diff !== 0) return diff
    }
    return 0
  }

  private getPlatformKey(): string {
    const map: Record<string, Record<string, string>> = {
      darwin: { x64: "darwin-x64", arm64: "darwin-arm64" },
      win32: { x64: "win32-x64", arm64: "win32-arm64" },
      linux: { x64: "linux-x64", arm64: "linux-arm64" },
    }
    return map[process.platform]?.[process.arch] ?? ""
  }
}
