import { EventBus } from "./events/bus"
import { createLogger } from "./logger"
import { AuthManager } from "./auth/manager"
import { resolveConfigLocation } from "./config/location"
import { SettingsService } from "./settings/service"
import { BinaryResolver } from "./settings/binaries"
import { FileSystemBrowser } from "./filesystem/browser"
import { WorkspaceManager } from "./workspaces/manager"
import { InstanceStore } from "./storage/instance-store"
import { InstanceEventBridge } from "./workspaces/instance-events"
import { SpeechService } from "./speech/service"
import { SideCarManager } from "./sidecars/manager"
import { ClientConnectionManager } from "./clients/connection-manager"
import { PluginChannelManager } from "./plugins/channel"
import { VoiceModeManager } from "./plugins/voice-mode"
import { RemoteProxySessionManager } from "./server/remote-proxy"
import { createHttpServer } from "./server/http-server"
import { resolveHttpsOptions } from "./server/tls"
import { ServerMeta } from "./api-types"
import { resolveUi } from "./ui/remote-ui"
import { startDevReleaseMonitor } from "./releases/dev-release-monitor"
import { readServerPackageVersion, resolveServerPublicDir } from "./runtime-paths"
import { isBinaryAvailable, triggerBinaryDownload } from "./opencode-paths"

export interface ServerOptions {
  port: number
  host?: string
  password?: string
  username?: string
  logLevel?: string
  configPath?: string
  rootDir?: string
  uiStaticDir?: string
  devServerUrl?: string
}

export interface ServerHandle {
  url: string
  stop: () => Promise<void>
}

export async function startServer(options: ServerOptions): Promise<ServerHandle> {
  const host = options.host ?? "127.0.0.1"
  const port = options.port
  const password = options.password
  const username = options.username ?? "embeddedcowork"
  const logLevel = options.logLevel ?? "info"
  const configPath = options.configPath ?? "~/.config/EmbeddedCowork/config.json"
  const rootDir = options.rootDir ?? process.cwd()
  const serverVersion = readServerPackageVersion(import.meta.url)

  const logger = createLogger({ level: logLevel, component: "app" })
  const workspaceLogger = logger.child({ component: "workspace" })
  const configLogger = logger.child({ component: "config" })
  const eventLogger = logger.child({ component: "events" })

  logger.info({ port, host }, "Starting EmbeddedCowork server (in-process)")

  const eventBus = new EventBus(eventLogger)
  const isLoopbackHost = (h: string) => h === "127.0.0.1" || h === "::1" || h.startsWith("127.")

  const configLocation = resolveConfigLocation(configPath)
  const authManager = new AuthManager(
    {
      configPath: configLocation.configYamlPath,
      username,
      password,
      cookieName: "embeddedcowork_session",
      generateToken: false,
      dangerouslySkipAuth: false,
    },
    logger.child({ component: "auth" }),
  )

  const serverMeta: ServerMeta = {
    localUrl: `http://127.0.0.1:${port}`,
    remoteUrl: undefined,
    eventsUrl: "/api/events",
    host,
    listeningMode: "local",
    localPort: port,
    remotePort: undefined,
    hostLabel: host,
    workspaceRoot: rootDir,
    addresses: [],
    serverVersion,
  }

  const settings = new SettingsService(configLocation, eventBus, configLogger)
  const binaryResolver = new BinaryResolver(settings)
  const workspaceManager = new WorkspaceManager({
    rootDir,
    settings,
    binaryResolver,
    eventBus,
    logger: workspaceLogger,
    getServerBaseUrl: () => serverMeta.localUrl,
  })
  const fileSystemBrowser = new FileSystemBrowser({ rootDir, unrestricted: true })
  const instanceStore = new InstanceStore(configLocation.instancesDir)
  const speechService = new SpeechService(settings, logger.child({ component: "speech" }))
  const sidecarManager = new SideCarManager({ settings, eventBus, logger: logger.child({ component: "sidecars" }) })
  const instanceEventBridge = new InstanceEventBridge({
    workspaceManager,
    eventBus,
    logger: logger.child({ component: "instance-events" }),
  })

  const uiResolution = await resolveUi({
    serverVersion,
    bundledUiDir: options.uiStaticDir ?? resolveServerPublicDir(import.meta.url),
    autoUpdate: false,
    overrideUiDir: undefined,
    uiDevServerUrl: options.devServerUrl,
    manifestUrl: undefined,
    logger: logger.child({ component: "ui" }),
  })

  serverMeta.ui = {
    version: uiResolution.uiVersion,
    source: uiResolution.source,
  }
  serverMeta.support = {
    supported: true,
    message: "",
    latestServerVersion: undefined,
    latestServerUrl: undefined,
    minServerVersion: undefined,
  }

  const clientConnectionManager = new ClientConnectionManager(logger.child({ component: "client-connections" }))
  const pluginChannel = new PluginChannelManager(logger.child({ component: "plugin-channel" }))
  const remoteProxySessionManager = new RemoteProxySessionManager({
    authManager,
    logger: logger.child({ component: "remote-proxy" }),
    httpsOptions: undefined,
  })
  const voiceModeManager = new VoiceModeManager({
    connections: clientConnectionManager,
    channel: pluginChannel,
    logger: logger.child({ component: "voice-mode" }),
  })

  const httpServer = createHttpServer({
    bindHost: "127.0.0.1",
    bindPort: port,
    defaultPort: port,
    protocol: "http",
    workspaceManager,
    settings,
    fileSystemBrowser,
    eventBus,
    serverMeta,
    instanceStore,
    speechService,
    sidecarManager,
    authManager,
    clientConnectionManager,
    pluginChannel,
    voiceModeManager,
    remoteProxySessionManager,
    binaryResolver,
    uiStaticDir: uiResolution.uiStaticDir ?? resolveServerPublicDir(import.meta.url),
    uiDevServerUrl: uiResolution.uiDevServerUrl,
    logger,
  })

  const serverStart = await httpServer.start()
  const localUrl = `http://127.0.0.1:${serverStart.port}`
  serverMeta.localUrl = localUrl
  serverMeta.localPort = serverStart.port

  logger.info({ url: localUrl }, "Server listening (in-process)")

  if (!isBinaryAvailable()) {
    triggerBinaryDownload(logger).catch(() => {})
  }

  return {
    url: localUrl,
    stop: async () => {
      await httpServer.stop()
      instanceEventBridge.shutdown()
      sidecarManager.shutdown()
      clientConnectionManager.shutdown()
      workspaceManager.shutdown()
    },
  }
}
