export const tailscaleMessages = {
  "tailscale.title": "Tailscale / Mesh VPN",
  "tailscale.description": "Connect EmbeddedCowork to your Tailscale/Headscale network for secure remote access.",

  "tailscale.status.connected": "Connected",
  "tailscale.status.disconnected": "Disconnected",
  "tailscale.status.connecting": "Connecting…",
  "tailscale.status.authNeeded": "Authentication required",

  "tailscale.auth.authKey.label": "Pre-auth Key",
  "tailscale.auth.authKey.placeholder": "tskey-auth-xxxxxxxxxxxx",
  "tailscale.auth.authKey.save": "Save & Connect",
  "tailscale.auth.authKey.success": "Auth key saved, reconnecting…",
  "tailscale.auth.authKey.error": "Failed to set auth key: {error}",

  "tailscale.auth.interactive.label": "Interactive Login",
  "tailscale.auth.interactive.open": "Open Login Page",
  "tailscale.auth.interactive.url": "Login URL",
  "tailscale.auth.interactive.copy": "Copy URL",
  "tailscale.auth.interactive.qr": "Show QR Code",

  "tailscale.ips": "Tailscale IPs",
  "tailscale.hostname": "Node Name",
  "tailscale.controlUrl": "Control Server",

  "tailscale.connect": "Enable Tailscale",
  "tailscale.disconnect": "Disconnect",
  "tailscale.enableSwitch": "Enable Tailscale integration",
  "tailscale.enableNote": "Requires a restart to take effect.",
  "tailscale.settings.controlUrl": "Control Server URL",
  "tailscale.settings.controlUrl.placeholder": "https://headscale.example.com",
  "tailscale.settings.hostname": "Node Hostname",
  "tailscale.settings.hostname.placeholder": "embeddedcowork-{name}",

  "tailscale.status.label": "Status",

  "tailscale.error.overview": "Error: {error}",
  "tailscale.copy": "Copy",
  "tailscale.refresh": "Refresh",
} as const
