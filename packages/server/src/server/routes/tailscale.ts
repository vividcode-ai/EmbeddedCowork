import type { FastifyInstance } from "fastify"
import { z } from "zod"
import type { Logger } from "../../logger"
import type { TailscaleIntegration } from "../tailscale-integration"
import type { SettingsService } from "../../settings/service"

interface RouteDeps {
  logger: Logger
  tailscaleIntegration?: TailscaleIntegration
  settings?: SettingsService
}

const AuthKeySchema = z.object({
  authKey: z.string().min(1, "authKey is required"),
})

const ControlUrlsSchema = z.object({
  urls: z.array(z.string().min(1)).min(1, "at least one URL is required"),
  activeUrl: z.string().min(1, "activeUrl is required"),
})

export function registerTailscaleRoutes(app: FastifyInstance, deps: RouteDeps) {
  if (!deps.tailscaleIntegration) {
    return
  }

  app.get("/api/tailscale/status", async () => {
    const status = await deps.tailscaleIntegration!.getStatus()
    return {
      ok: status.ok,
      connected: status.connected,
      tailscaleIPs: status.tailscaleIPs ?? [],
      hostname: status.hostname ?? "",
      authNeeded: status.authNeeded ?? false,
      authMethod: status.authMethod ?? "none",
      loginURL: status.loginURL,
      online: status.online ?? false,
      error: status.error,
    }
  })

  app.post("/api/tailscale/auth-key", async (request, reply) => {
    try {
      const body = AuthKeySchema.parse(request.body ?? {})
      const result = await deps.tailscaleIntegration!.setAuthKey(body.authKey)

      if (!result.ok) {
        reply.code(500)
        return { ok: false, error: result.error ?? "failed to set auth key" }
      }

      return { ok: true }
    } catch (error) {
      deps.logger.warn({ err: error }, "failed to set tailscale auth key")
      reply.code(400)
      return { ok: false, error: error instanceof Error ? error.message : "invalid request" }
    }
  })

  app.get("/api/tailscale/login-url", async () => {
    const result = await deps.tailscaleIntegration!.getLoginURL()
    return {
      ok: result.ok,
      url: result.url,
      error: result.error,
    }
  })

  app.post("/api/tailscale/stop", async () => {
    await deps.tailscaleIntegration!.stop()
    return { ok: true }
  })

  app.post("/api/tailscale/start", async () => {
    await deps.tailscaleIntegration!.start()
    const status = await deps.tailscaleIntegration!.getStatus()
    return { ok: status.ok, status }
  })

  app.get("/api/tailscale/control-urls", async () => {
    const config = (deps.settings?.getOwner("config", "server").tailscale ?? {}) as {
      controlUrls?: string[]
      activeControlUrl?: string
    }
    return {
      urls: config.controlUrls ?? [],
      activeUrl: config.activeControlUrl ?? "",
    }
  })

  app.put("/api/tailscale/control-urls", async (request, reply) => {
    try {
      const body = ControlUrlsSchema.parse(request.body ?? {})

      if (!body.urls.includes(body.activeUrl)) {
        reply.code(400)
        return { ok: false, error: "activeUrl must be in the urls list" }
      }

      deps.settings?.mergePatchOwner("config", "server", {
        tailscale: { controlUrls: body.urls, activeControlUrl: body.activeUrl },
      })

      await deps.tailscaleIntegration!.setControlUrl(body.activeUrl)

      return { ok: true }
    } catch (error) {
      deps.logger.warn({ err: error }, "failed to set tailscale control URLs")
      reply.code(400)
      return { ok: false, error: error instanceof Error ? error.message : "invalid request" }
    }
  })
}
