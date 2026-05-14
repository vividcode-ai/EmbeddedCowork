import crypto from "node:crypto"

export const OPENCODE_SERVER_USERNAME_ENV = "OPENCODE_SERVER_USERNAME" as const
export const OPENCODE_SERVER_PASSWORD_ENV = "OPENCODE_SERVER_PASSWORD" as const

export const DEFAULT_OPENCODE_USERNAME = "embeddedcowork" as const

export function generateOpencodeServerPassword(): string {
  return crypto.randomBytes(32).toString("base64url")
}

export function buildOpencodeBasicAuthHeader(params: { username?: string; password?: string }): string | undefined {
  const username = params.username
  const password = params.password

  if (!username || !password) {
    return undefined
  }

  const token = Buffer.from(`${username}:${password}`, "utf8").toString("base64")
  return `Basic ${token}`
}

export function resolveOpencodeServerAuth(options?: {
  userEnvironment?: Record<string, string | undefined>
}): { username: string; password: string; authorization: string } {
  const userEnv = options?.userEnvironment ?? {}

  const username = userEnv[OPENCODE_SERVER_USERNAME_ENV]
    ?? process.env[OPENCODE_SERVER_USERNAME_ENV]
    ?? DEFAULT_OPENCODE_USERNAME

  const password = userEnv[OPENCODE_SERVER_PASSWORD_ENV]
    ?? process.env[OPENCODE_SERVER_PASSWORD_ENV]
    ?? generateOpencodeServerPassword()

  const authorization = buildOpencodeBasicAuthHeader({ username, password })
  if (!authorization) {
    throw new Error("Failed to build OpenCode auth header")
  }

  return { username, password, authorization }
}
