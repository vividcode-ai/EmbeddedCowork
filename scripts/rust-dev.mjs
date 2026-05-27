import { spawn, execSync } from "child_process";
import { existsSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

function log(tag, message) {
  console.log(`[${tag}] ${message}`);
}

// Check if npm dependencies are installed
if (!existsSync(resolve(root, "node_modules"))) {
  log("setup", "Installing dependencies (first run)...");
  try {
    execSync("npm install", { cwd: root, stdio: "inherit", timeout: 180000 });
  } catch (e) {
    console.error("Failed to install dependencies");
    process.exit(1);
  }
}

// Build the Rust server
log("build", "Building Rust server...");
try {
  execSync("cargo build --manifest-path packages/server-rust/Cargo.toml", {
    cwd: root,
    stdio: "inherit",
    timeout: 300000,
  });
} catch (e) {
  console.error("Failed to build Rust server");
  process.exit(1);
}

// Start the Electron app with RUST_SERVER env
log("electron", "Starting Electron app with Rust server...");
const electron = spawn(
  "npm",
  ["run", "dev", "--workspace", "@vividcodeAI/embeddedcowork-electron-app"],
  {
    cwd: root,
    stdio: "inherit",
    shell: true,
    env: {
      ...process.env,
      EMBEDDEDCOWORK_RUST_SERVER: "1",
    },
  }
);

electron.on("exit", (code) => {
  process.exit(code ?? 0);
});

process.on("SIGINT", () => {
  electron.kill();
  process.exit(0);
});

process.on("SIGTERM", () => {
  electron.kill();
  process.exit(0);
});
