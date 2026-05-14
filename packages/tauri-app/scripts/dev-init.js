#!/usr/bin/env node
const fs = require("fs")
const path = require("path")
const serverDir = path.resolve(__dirname, "..", "src-tauri", "resources", "server")
if (!fs.existsSync(serverDir)) {
  fs.mkdirSync(serverDir, { recursive: true })
}
