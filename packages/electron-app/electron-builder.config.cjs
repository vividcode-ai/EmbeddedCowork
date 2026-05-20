/** @type {import("electron-builder").Configuration} */
const config = {
  appId: "ai.vividcode.embeddedcowork.client",
  productName: "EmbeddedCowork",
  publish: {
    provider: "github",
    owner: "vividcode-ai",
    repo: "EmbeddedCowork",
    releaseType: "release",
  },
  directories: {
    output: "release",
    buildResources: "electron/resources",
  },
  files: [
    "dist/**/*",
    "package.json",
  ],
  extraResources: [
    {
      from: "electron/resources",
      to: "",
      filter: [
        "!icon.icns",
        "!icon.ico",
      ],
    },
    {
      from: "../server/dist/opencode-config",
      to: "opencode-config",
    },
  ],
  mac: {
    entitlements: "electron/resources/entitlements.mac.plist",
    entitlementsInherit: "electron/resources/entitlements.mac.plist",
    extendInfo: {
      NSMicrophoneUsageDescription: "EmbeddedCowork needs microphone access for speech-to-text prompt input.",
      NSLocalNetworkUsageDescription: "EmbeddedCowork needs local network access to connect to locally hosted AI and speech services.",
    },
    category: "public.app-category.developer-tools",
    target: [
      {
        target: "zip",
        arch: [
          "x64",
          "arm64",
        ],
      },
    ],
    artifactName: "EmbeddedCowork-${version}-${os}-${arch}.${ext}",
    icon: "electron/resources/icon.icns",
  },
  dmg: {
    contents: [
      {
        x: 130,
        y: 220,
      },
      {
        x: 410,
        y: 220,
        type: "link",
        path: "/Applications",
      },
    ],
  },
  win: {
    target: [
      {
        target: "nsis",
        arch: [
          "x64",
          "arm64",
        ],
      },
      {
        target: "zip",
        arch: [
          "x64",
          "arm64",
        ],
      },
    ],
    artifactName: "EmbeddedCowork-${version}-${os}-${arch}.${ext}",
    icon: "electron/resources/icon.ico",
  },
  nsis: {
    oneClick: false,
    allowToChangeInstallationDirectory: true,
    createDesktopShortcut: true,
    createStartMenuShortcut: true,
  },
  linux: {
    target: [
      {
        target: "zip",
        arch: [
          "x64",
          "arm64",
        ],
      },
      {
        target: "AppImage",
        arch: [
          "x64",
          "arm64",
        ],
      },
    ],
    artifactName: "EmbeddedCowork-${version}-${os}-${arch}.${ext}",
    category: "Development",
    icon: "electron/resources/icon.png",
  },
}

module.exports = config
