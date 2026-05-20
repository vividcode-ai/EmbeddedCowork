// electron.vite.config.ts
import { defineConfig, externalizeDepsPlugin } from "electron-vite";
import solid from "vite-plugin-solid";
import { resolve as resolve2 } from "path";

// ../ui/scripts/monaco-public-assets.js
import fs from "fs";
import { resolve } from "path";
function copyMonacoPublicAssets(params) {
  const uiRendererRoot2 = params?.uiRendererRoot;
  if (!uiRendererRoot2) {
    throw new Error("copyMonacoPublicAssets: uiRendererRoot is required");
  }
  const warn = params?.warn ?? ((message) => console.warn(message));
  const publicDir = resolve(uiRendererRoot2, "public");
  const destRoot = resolve(publicDir, "monaco/vs");
  const candidates = params?.sourceRoots?.length > 0 ? params.sourceRoots : [
    // Workspace root hoisted deps.
    resolve(process.cwd(), "node_modules/monaco-editor/min/vs"),
    // UI package local deps (covers non-hoisted installs).
    resolve(process.cwd(), "packages/ui/node_modules/monaco-editor/min/vs")
  ];
  const sourceRoot = candidates.find((p) => fs.existsSync(resolve(p, "loader.js")));
  if (!sourceRoot) {
    warn("Monaco source directory not found; skipping copy");
    return;
  }
  const copyRecursive = (src, dest) => {
    const stat = fs.statSync(src);
    if (stat.isDirectory()) {
      fs.mkdirSync(dest, { recursive: true });
      for (const entry of fs.readdirSync(src)) {
        copyRecursive(resolve(src, entry), resolve(dest, entry));
      }
      return;
    }
    fs.copyFileSync(src, dest);
  };
  try {
    fs.rmSync(destRoot, { recursive: true, force: true });
  } catch {
  }
  fs.mkdirSync(destRoot, { recursive: true });
  for (const dir of ["base", "editor", "platform"]) {
    const src = resolve(sourceRoot, dir);
    if (fs.existsSync(src)) {
      copyRecursive(src, resolve(destRoot, dir));
    }
  }
  copyRecursive(resolve(sourceRoot, "loader.js"), resolve(destRoot, "loader.js"));
  for (const lang of ["typescript", "html", "json", "css"]) {
    const src = resolve(sourceRoot, "language", lang);
    if (fs.existsSync(src)) {
      copyRecursive(src, resolve(destRoot, "language", lang));
    }
  }
  for (const lang of ["python", "markdown", "cpp", "kotlin"]) {
    const src = resolve(sourceRoot, "basic-languages", lang);
    if (fs.existsSync(src)) {
      copyRecursive(src, resolve(destRoot, "basic-languages", lang));
    }
  }
  const monacoContribution = resolve(sourceRoot, "basic-languages", "monaco.contribution.js");
  if (fs.existsSync(monacoContribution)) {
    copyRecursive(monacoContribution, resolve(destRoot, "basic-languages", "monaco.contribution.js"));
  }
  const underscoreContribution = resolve(sourceRoot, "basic-languages", "_.contribution.js");
  if (fs.existsSync(underscoreContribution)) {
    copyRecursive(underscoreContribution, resolve(destRoot, "basic-languages", "_.contribution.js"));
  }
}

// electron.vite.config.ts
var __electron_vite_injected_dirname = "D:\\autoway\\Project\\agent\\embedded\\EmbeddedCowork\\packages\\electron-app";
var uiRoot = resolve2(__electron_vite_injected_dirname, "../ui");
var uiSrc = resolve2(uiRoot, "src");
var uiRendererRoot = resolve2(uiRoot, "src/renderer");
var uiRendererEntry = resolve2(uiRendererRoot, "index.html");
var uiRendererLoadingEntry = resolve2(uiRendererRoot, "loading.html");
function prepareMonacoPublicAssets() {
  return {
    name: "prepare-monaco-public-assets",
    configureServer(server) {
      copyMonacoPublicAssets({
        uiRendererRoot,
        warn: (msg) => server.config.logger.warn(msg),
        sourceRoots: [
          resolve2(__electron_vite_injected_dirname, "../../node_modules/monaco-editor/min/vs"),
          resolve2(uiRoot, "node_modules/monaco-editor/min/vs")
        ]
      });
    },
    buildStart() {
      copyMonacoPublicAssets({
        uiRendererRoot,
        warn: (msg) => this.warn(msg),
        sourceRoots: [
          resolve2(__electron_vite_injected_dirname, "../../node_modules/monaco-editor/min/vs"),
          resolve2(uiRoot, "node_modules/monaco-editor/min/vs")
        ]
      });
    }
  };
}
var electron_vite_config_default = defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    build: {
      outDir: "dist/main",
      lib: {
        entry: resolve2(__electron_vite_injected_dirname, "electron/main/main.ts")
      },
      rollupOptions: {
        external: ["electron"]
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    build: {
      outDir: "dist/preload",
      lib: {
        entry: resolve2(__electron_vite_injected_dirname, "electron/preload/index.cjs"),
        formats: ["cjs"],
        fileName: () => "index.js"
      },
      rollupOptions: {
        external: ["electron"],
        output: {
          entryFileNames: "index.js"
        }
      }
    }
  },
  renderer: {
    root: uiRendererRoot,
    plugins: [solid(), prepareMonacoPublicAssets()],
    css: {
      postcss: resolve2(uiRoot, "postcss.config.js")
    },
    resolve: {
      alias: {
        "@": uiSrc
      }
    },
    server: {
      port: 3e3
    },
    build: {
      minify: false,
      cssMinify: false,
      sourcemap: true,
      outDir: resolve2(__electron_vite_injected_dirname, "dist/renderer"),
      rollupOptions: {
        input: {
          main: uiRendererEntry,
          loading: uiRendererLoadingEntry
        },
        output: {
          compact: false,
          minifyInternalExports: false
        }
      }
    }
  }
});
export {
  electron_vite_config_default as default
};
