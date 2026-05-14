const { execSync } = require("child_process")
const path = require("path")

const ext = process.platform === "win32" ? ".exe" : ""
const cwd = path.join(__dirname, "..", "packages", "tailscale-sidecar")
const out = `bin/tailscale-sidecar${ext}`

console.log(`[build-tailscale-sidecar] building for ${process.platform} → ${out}`)
execSync(`go build -o ${out} .`, { cwd, stdio: "inherit" })
