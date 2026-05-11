import { FastifyInstance } from "fastify"
import type { SettingsService } from "../../settings/service"
import type { Logger } from "../../logger"
import { getOpencodeStatus, triggerBinaryDownload } from "../../opencode-paths"

export function registerOpencodeStatusRoutes(app: FastifyInstance, deps: { settings: SettingsService; logger: Logger }) {
  app.get("/api/opencode/status", async (_request, reply) => {
    const status = getOpencodeStatus()

    if (!status.available) {
      void triggerBinaryDownload(deps.logger)
    }

    return reply.send(status)
  })
}
