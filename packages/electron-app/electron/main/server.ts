type ServerHandle = {
  url: string
  stop: () => Promise<void>
}

let serverHandle: ServerHandle | null = null

export async function startInProcessServer(options: {
  port: number
  password: string
  logLevel?: string
}): Promise<ServerHandle> {
  // In dev mode, use the virtual module defined in electron.vite.config.ts.
  // In production, resolve from the workspace dependency at runtime.
  let startServer: (opts: {
    port: number
    host?: string
    password?: string
    logLevel?: string
  }) => Promise<ServerHandle>

  const isDev = !process.env.NODE_ENV || process.env.NODE_ENV === "development"
  if (isDev) {
    const mod = await import("virtual:embeddedcowork-server")
    startServer = mod.startServer
  } else {
    const mod = await import("@vividcodeai/embeddedcowork/dist/node/node-entry.mjs")
    startServer = mod.startServer
  }

  serverHandle = await startServer({
    port: options.port,
    host: "127.0.0.1",
    password: options.password,
    logLevel: options.logLevel ?? "info",
  })
  return serverHandle
}

export function getServerHandle(): ServerHandle | null {
  return serverHandle
}

export async function stopServer(): Promise<void> {
  if (serverHandle) {
    await serverHandle.stop()
    serverHandle = null
  }
}
