package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
)

func main() {
	var (
		apiPort    int
		stateDir   string
		hostname   string
		controlURL string
		authKey    string
	)

	flag.IntVar(&apiPort, "api-port", 0, "HTTP API port for sidecar management (0 = auto-assign)")
	flag.StringVar(&stateDir, "state-dir", "", "Directory for tsnet state storage")
	flag.StringVar(&hostname, "hostname", "", "Tailscale node hostname")
	flag.StringVar(&controlURL, "control-url", "", "Tailscale control server URL (default: https://control.tailscale.com)")
	flag.StringVar(&authKey, "auth-key", "", "Pre-auth key for automatic registration")
	flag.Parse()

	if hostname == "" {
		hostname = "embeddedcowork"
	}

	if stateDir == "" {
		execPath, err := os.Executable()
		if err == nil {
			stateDir = filepath.Join(filepath.Dir(execPath), "tsnet-state")
		} else {
			stateDir = "./tsnet-state"
		}
	}

	if err := os.MkdirAll(stateDir, 0755); err != nil {
		log.Fatalf("failed to create state dir %s: %v", stateDir, err)
	}

	if controlURL == "" {
		controlURL = os.Getenv("TS_CONTROL_URL")
	}
	if authKey == "" {
		authKey = os.Getenv("TS_AUTHKEY")
	}

	log.Printf("starting tailscale sidecar")
	log.Printf("  hostname:   %s", hostname)
	log.Printf("  state-dir:  %s", stateDir)
	if controlURL != "" {
		log.Printf("  control:    %s", controlURL)
	}
	log.Printf("  api-port:   %d", apiPort)

	node := NewTailscaleNode(stateDir, hostname, controlURL, authKey)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	if err := node.Up(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: failed to start tailscale: %v\n", err)
		os.Exit(1)
	}

	if err := startAPIServer(node, apiPort); err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: api server failed: %v\n", err)
		os.Exit(1)
	}
}
