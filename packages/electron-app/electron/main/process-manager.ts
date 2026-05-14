import { spawn, spawnSync, type ChildProcess } from "child_process"
import { createServer as createNetServer } from "net"
import { app, utilityProcess, type UtilityProcess } from "electron"
import { createRequire } from "module"
import { EventEmitter } from "events"
import { existsSync, mkdirSync, readFileSync } from "fs"
import os from "os"
import path from "path"
import { fileURLToPath } from "url"
import { parse as parseYaml } from "yaml"
import { buildUserShellCommand, getUserShellEnv, supportsUserShell } from "./user-shell"

const nodeRequire = createRequire(import.meta.url)
const mainFilename = fileURLToPath(import.meta.url)
const mainDirname = path.dirname(mainFilename)

type CliState = "starting" | "ready" | "error" | "stopped"
type ListeningMode = "local" | "all"

export interface CliStatus {
  state: CliState
  pid?: number
  port?: number
  url?: string
  error?: string
}

interface StartOptions {
  dev: boolean
  inProcess?: boolean
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
  on(event: "exit", listener: (status: CliStatus) => void): this
  on(event: "error", listener: (error: Error) => void): this
}

const MAX_CAPTURED_LINES = 50

export class CliProcessManager extends EventEmitter {
  private child?: ManagedChild
  private childLaunchMode: ChildLaunchMode = "spawn"
  private status: CliStatus = { state: "stopped" }
  private requestedStop = false
  private capturedOutput: string[] = []

  async start(options: StartOptions): Promise<CliStatus> {
    if (this.child) {
      await this.stop()
    }

    this.requestedStop = false
    this.capturedOutput = []
    this.updateStatus({ state: "starting", port: undefined, pid: undefined, url: undefined, error: undefined })

    if (options.inProcess) {
      return this.startInProcess(options)
    }

    const listeningMode = this.resolveListeningMode()
    const host = resolveHostForMode(listeningMode)
    const port = await this.findFreePort()
    const password = this.generatePassword()
    const args = this.buildCliArgs(options, host, port, password)
    const cliEntry = this.resolveCliEntry(options)

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

    const baseUrl = `http://127.0.0.1:${port}`
    child.stdout?.on("data", (data: Buffer) => {
      const raw = data.toString()
      const lines = raw.split("\n").filter((l) => l.trim())
      this.captureOutput(raw)
      for (const line of lines) {
        console.info(`[cli][stdout] ${line.trim()}`)
      }
    })
    child.stdout?.on("error", (err) => {
      if ((err as NodeJS.ErrnoException).code !== "EPIPE") {
        console.error("[cli] stdout stream error:", err)
      }
    })

    child.stderr?.on("data", (data: Buffer) => {
      const raw = data.toString()
      const lines = raw.split("\n").filter((l) => l.trim())
      this.captureOutput(raw)
      for (const line of lines) {
        console.info(`[cli][stderr] ${line.trim()}`)
      }
    })
    child.stderr?.on("error", (err) => {
      if ((err as NodeJS.ErrnoException).code !== "EPIPE") {
        console.error("[cli] stderr stream error:", err)
      }
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
        const error = failed ? this.status.error ?? `CLI exited with code ${code ?? 0}${this.formatLastOutput()}` : undefined
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
        const error = failed ? this.status.error ?? `CLI exited with code ${code ?? 0}${signal ? ` (${signal})` : ""}${this.formatLastOutput()}` : undefined
        console.info(`[cli] exit (code=${code}, signal=${signal || ""})${error ? ` error=${error}` : ""}`)
        this.updateStatus({ state: failed ? "error" : "stopped", error })
        if (failed && error) {
          this.emit("error", new Error(error))
        }
        this.emit("exit", this.status)
        this.child = undefined
      })
    }

    // Health check + login in background
    this.healthCheckAndLogin(baseUrl, password).then((ok) => {
      if (ok) {
        console.info(`[cli] ready on ${baseUrl}`)
        this.updateStatus({ state: "ready", port, url: baseUrl })
        this.emit("ready", this.status)
      } else {
        console.warn(`[cli] health check failed, navigating to login page`)
        this.updateStatus({ state: "ready", port, url: baseUrl })
        this.emit("ready", this.status)
      }
    })

    return new Promise<CliStatus>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.handleTimeout()
        reject(new Error("CLI startup timeout"))
      }, 60000)

      this.once("ready", (status) => {
        clearTimeout(timeout)
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
      // In-process mode or already stopped
      try {
        const { stopServer } = await import("./server")
        await stopServer()
      } catch {
        // not running in-process, ignore
      }
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

  private updateStatus(patch: Partial<CliStatus>) {
    this.status = { ...this.status, ...patch }
    this.emit("status", this.status)
  }

  private buildCliArgs(options: StartOptions, host: string, port: number, password: string): string[] {
    const args = [
      "serve", "--host", host,
      "--password", password,
      "--http-port", String(port),
      "--http", "true",
      "--unrestricted-root",
    ]

    if (options.dev) {
      args.push("--https", "false")
      const devServer = process.env.VITE_DEV_SERVER_URL || process.env.ELECTRON_RENDERER_URL || "http://localhost:3000"
      const rawLogLevel = (process.env.CLI_LOG_LEVEL ?? "info").trim()
      const logLevel = rawLogLevel.length > 0 ? rawLogLevel.toLowerCase() : "info"
      args.push("--ui-dev-server", devServer, "--log-level", logLevel)
    } else {
      args.push("--https", "false")
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

  private resolveCliEntry(options: StartOptions): CliEntryResolution {
    if (options.dev) {
      const tsxPath = this.resolveTsx()
      if (!tsxPath) {
        throw new Error("tsx is required to run the CLI in development mode. Please install dependencies.")
      }
      const devEntry = this.resolveDevEntry()
      return { entry: devEntry, runner: "tsx", runnerPath: tsxPath }
    }

    // Prod: 三步检测链
    const systemEntry = this.resolveSystemEntry()
    if (systemEntry) {
      return { entry: systemEntry, runner: "system" }
    }

    return { entry: this.resolveStandaloneProdEntry(), runner: "standalone" }
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
   *   1. which/where → "embeddedCowork" on PATH (npm 全局安装)
   *   2. ~/.embeddedcowork/bin/embeddedcowork-server (CI 定义的 binary 名)
   *   3. 未找到 → return null
   */
  private resolveSystemEntry(): string | null {
    const locator = process.platform === "win32" ? "where" : "which"
    try {
      const result = spawnSync(locator, ["embeddedCowork"], { encoding: "utf8" })
      if (result.status === 0 && result.stdout) {
        const candidates = result.stdout
          .split(/\r?\n/)
          .map((line: string) => line.trim())
          .filter((line: string) => line.length > 0)
          .filter((line: string) => !/^INFO:/i.test(line))
        if (candidates.length > 0 && existsSync(candidates[0])) {
          return candidates[0]
        }
      }
    } catch {}

    const binDir = path.join(os.homedir(), ".embeddedcowork", "bin")
    const binaryName = process.platform === "win32" ? "embeddedcowork-server.exe" : "embeddedcowork-server"
    const installedPath = path.join(binDir, binaryName)
    if (existsSync(installedPath)) {
      return installedPath
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

  private async startInProcess(options: StartOptions): Promise<CliStatus> {
    try {
      const port = await this.findFreePort()
      const password = this.generatePassword()
      const baseUrl = `http://127.0.0.1:${port}`

      console.info(`[cli] starting in-process server on ${baseUrl}`)
      const { startInProcessServer } = await import("./server")
      const handle = await startInProcessServer({ port, password, logLevel: options.dev ? "info" : "warn" })

      this.updateStatus({ state: "ready", port, url: baseUrl })
      this.emit("ready", this.status)

      return this.status
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      console.error("[cli] in-process server start failed:", message)
      this.updateStatus({ state: "error", error: message })
      this.emit("error", error instanceof Error ? error : new Error(message))
      throw error
    }
  }

  private async findFreePort(): Promise<number> {
    return new Promise((resolve, reject) => {
      const server = createNetServer()
      server.listen(0, "127.0.0.1", () => {
        const address = server.address()
        if (address && typeof address === "object") {
          const port = address.port
          server.close(() => resolve(port))
        } else {
          server.close(() => reject(new Error("Failed to get port")))
        }
      })
      server.on("error", reject)
    })
  }

  private generatePassword(): string {
    return `${Date.now().toString(16)}_${process.pid.toString(16)}`
  }

  private async healthCheckAndLogin(baseUrl: string, password: string): Promise<boolean> {
    const healthUrl = `${baseUrl}/api/health`
    const loginUrl = `${baseUrl}/api/auth/login`
    const startTime = Date.now()
    const timeout = 60000

    while (Date.now() - startTime < timeout) {
      try {
        const healthResp = await fetch(healthUrl)
        if (!healthResp.ok) {
          await this.sleep(200)
          continue
        }

        const loginResp = await fetch(loginUrl, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ username: "embeddedcowork", password }),
        })

        if (!loginResp.ok) {
          console.warn(`[cli] login returned ${loginResp.status}`)
          return false
        }

        const setCookieHeader = loginResp.headers.get("set-cookie")
        if (setCookieHeader) {
          const cookieParts = setCookieHeader.split(";")[0].trim()
          const eqIdx = cookieParts.indexOf("=")
          if (eqIdx > 0) {
            const name = cookieParts.slice(0, eqIdx).trim()
            const value = cookieParts.slice(eqIdx + 1).trim()
            const { session } = await import("electron")
            await session.defaultSession.cookies.set({
              url: baseUrl,
              name,
              value,
              httpOnly: true,
              path: "/",
              sameSite: "lax" as const,
            })
          }
        }

        return true
      } catch (error) {
        await this.sleep(200)
      }
    }

    return false
  }

  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms))
  }

  private captureOutput(text: string) {
    if (this.capturedOutput.length >= MAX_CAPTURED_LINES) {
      this.capturedOutput.shift()
    }
    this.capturedOutput.push(text)
  }

  private formatLastOutput(): string {
    if (this.capturedOutput.length === 0) return ""
    const tail = this.capturedOutput.slice(-20).join("").trim()
    if (tail.length === 0) return ""
    return `\nLast output:\n${tail.slice(0, 2000)}`
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

}
