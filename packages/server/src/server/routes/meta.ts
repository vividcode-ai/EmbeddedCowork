import { FastifyInstance } from "fastify"
import { ServerMeta } from "../../api-types"
import { BinaryResolver } from "../../settings/binaries"
import { resolveBinary, getDownloadPromise } from "../../opencode-paths"
import { probeBinaryVersion } from "../../workspaces/spawn"


interface RouteDeps {
  serverMeta: ServerMeta
  binaryResolver: BinaryResolver
}

export function registerMetaRoutes(app: FastifyInstance, deps: RouteDeps) {
  app.get("/api/meta", async () => buildMetaResponse(deps.serverMeta, deps.binaryResolver))
}

async function buildMetaResponse(meta: ServerMeta, binaryResolver: BinaryResolver): Promise<ServerMeta> {
  const localPort = resolveLocalPort(meta)
  const remote = resolveRemote(meta)

  if (!meta.opencodeVersion) {
    await probeOpencodeVersion(meta, binaryResolver)
  }

  return {
    ...meta,
    localPort,
    remotePort: remote?.port,
    listeningMode: meta.host === "0.0.0.0" || !isLoopbackHost(meta.host) ? "all" : "local",
  }
}

async function probeOpencodeVersion(meta: ServerMeta, resolver: BinaryResolver) {
  try {
    const defaultBinary = resolver.resolveDefault()
    let binaryPath = resolveBinary(defaultBinary.path)
    let result = probeBinaryVersion(binaryPath)

    if (!result.valid || !result.version) {
      const dlPromise = getDownloadPromise()
      if (dlPromise) {
        console.log("[meta] opencode not found, waiting for download...")
        await dlPromise
        binaryPath = resolveBinary(defaultBinary.path)
        result = probeBinaryVersion(binaryPath)
        console.log("[meta] after download, probe result:", { valid: result.valid, version: result.version })
      } else {
        console.log("[meta] opencode not found and no pending download, binaryPath:", binaryPath, "error:", result.error)
      }
    }

    if (result.valid && result.version) {
      meta.opencodeVersion = result.version
    }
  } catch (error) {
    console.log("[meta] probeOpencodeVersion threw:", error)
  }
}

function resolveLocalPort(meta: ServerMeta): number {
  if (Number.isInteger(meta.localPort) && meta.localPort > 0) {
    return meta.localPort
  }
  try {
    const parsed = new URL(meta.localUrl)
    const port = Number(parsed.port)
    return Number.isInteger(port) && port > 0 ? port : 0
  } catch {
    return 0
  }
}

function resolveRemote(meta: ServerMeta): { protocol: "http" | "https"; port: number } | null {
  if (!meta.remoteUrl) {
    return null
  }
  try {
    const parsed = new URL(meta.remoteUrl)
    const protocol = parsed.protocol === "https:" ? "https" : "http"
    const port = Number(parsed.port)
    return { protocol, port: Number.isInteger(port) && port > 0 ? port : 0 }
  } catch {
    return null
  }
}

function isLoopbackHost(host: string): boolean {
  return host === "127.0.0.1" || host === "::1" || host.startsWith("127.")
}

// NetworkAddress shape is resolved in ../network-addresses
