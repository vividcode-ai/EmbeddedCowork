const { chmodSync } = require("fs")
const { join } = require("path")
try {
  chmodSync(join(__dirname, "bin", "embeddedcowork-server"), 0o755)
} catch {}
