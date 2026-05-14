package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"

	"tailscale.com/tsnet"
)

type NodeStatus struct {
	Connected    bool     `json:"connected"`
	TailscaleIPs []string `json:"tailscaleIPs,omitempty"`
	Hostname     string   `json:"hostname,omitempty"`
	AuthNeeded   bool     `json:"authNeeded"`
	LoginURL     string   `json:"loginURL,omitempty"`
	AuthMethod   string   `json:"authMethod,omitempty"`
	Error        string   `json:"error,omitempty"`
	Online       bool     `json:"online"`
}

type TailscaleNode struct {
	server       *tsnet.Server
	mu           sync.RWMutex
	status       NodeStatus
	stateDir     string
	hostname     string
	controlURL   string
	authKey      string
	upCancel     context.CancelFunc
	forwardCancel context.CancelFunc
}

func NewTailscaleNode(stateDir, hostname, controlURL, authKey string) *TailscaleNode {
	return &TailscaleNode{
		stateDir:   stateDir,
		hostname:   hostname,
		controlURL: controlURL,
		authKey:    authKey,
		status: NodeStatus{
			AuthNeeded: authKey == "",
		},
	}
}

func (n *TailscaleNode) Up(ctx context.Context) error {
	n.mu.Lock()

	ctx, cancel := context.WithCancel(ctx)
	n.upCancel = cancel

	n.server = &tsnet.Server{
		Dir:        n.stateDir,
		Hostname:   n.hostname,
		ControlURL: n.controlURL,
		Ephemeral:  false,
		Logf:       n.capturingLogf(),
	}

	if n.authKey != "" {
		n.server.AuthKey = n.authKey
		n.status.AuthMethod = "authkey"
		n.status.AuthNeeded = false
	} else {
		n.status.AuthMethod = "interactive"
		n.status.AuthNeeded = true
	}

	n.mu.Unlock()

	go n.monitorConnection(ctx)

	go func() {
		if _, err := n.server.Up(ctx); err != nil && err != context.Canceled {
			n.mu.Lock()
			n.status.Error = fmt.Sprintf("tsnet up: %v", err)
			n.mu.Unlock()
		}
	}()

	return nil
}

func (n *TailscaleNode) Down() error {
	n.StopForwarder()

	n.mu.Lock()
	defer n.mu.Unlock()

	if n.upCancel != nil {
		n.upCancel()
		n.upCancel = nil
	}

	if n.server == nil {
		return nil
	}

	if err := n.server.Close(); err != nil {
		return fmt.Errorf("tsnet close: %w", err)
	}

	n.server = nil
	n.status = NodeStatus{}
	return nil
}

func (n *TailscaleNode) SetAuthKey(ctx context.Context, authKey string) error {
	n.mu.Lock()
	n.authKey = authKey
	n.mu.Unlock()

	if err := n.Down(); err != nil {
		return err
	}

	return n.Up(ctx)
}

func (n *TailscaleNode) Status() NodeStatus {
	n.mu.RLock()
	defer n.mu.RUnlock()
	return n.status
}

func (n *TailscaleNode) GetLoginURL() (string, error) {
	n.mu.RLock()
	defer n.mu.RUnlock()

	if n.server == nil {
		return "", fmt.Errorf("tsnet not started")
	}

	if n.status.LoginURL == "" {
		return "", fmt.Errorf("login URL not yet available")
	}

	return n.status.LoginURL, nil
}

func (n *TailscaleNode) Listen(ctx context.Context, localPort int) ([]string, int, net.Listener, error) {
	n.mu.RLock()
	server := n.server
	n.mu.RUnlock()

	if server == nil {
		return nil, 0, nil, fmt.Errorf("tsnet not started")
	}

	ln, err := server.Listen("tcp", fmt.Sprintf(":%d", localPort))
	if err != nil {
		return nil, 0, nil, fmt.Errorf("tsnet listen: %w", err)
	}

	ips := n.Status().TailscaleIPs
	addr := ln.Addr().(*net.TCPAddr)

	go func() {
		<-ctx.Done()
		ln.Close()
	}()

	return ips, addr.Port, ln, nil
}

func (n *TailscaleNode) StartForwarder(ctx context.Context, listenPort int, targetHost string, targetPort int) error {
	n.mu.Lock()
	defer n.mu.Unlock()

	if n.forwardCancel != nil {
		return fmt.Errorf("forwarder already running")
	}
	if n.server == nil {
		return fmt.Errorf("tsnet not started")
	}

	ln, err := n.server.Listen("tcp", fmt.Sprintf(":%d", listenPort))
	if err != nil {
		return fmt.Errorf("tsnet listen: %w", err)
	}

	ctx, cancel := context.WithCancel(ctx)
	n.forwardCancel = cancel

	go func() {
		<-ctx.Done()
		ln.Close()
	}()

	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go forwardConn(conn, targetHost, targetPort)
		}
	}()

	return nil
}

func forwardConn(remote net.Conn, host string, port int) {
	defer remote.Close()
	target := net.JoinHostPort(host, strconv.Itoa(port))
	local, err := net.DialTimeout("tcp", target, 10*time.Second)
	if err != nil {
		log.Printf("forward: dial %s: %v", target, err)
		return
	}
	defer local.Close()

	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		io.Copy(local, remote)
	}()
	go func() {
		defer wg.Done()
		io.Copy(remote, local)
	}()
	wg.Wait()
}

func (n *TailscaleNode) StopForwarder() {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.forwardCancel != nil {
		n.forwardCancel()
		n.forwardCancel = nil
	}
}

func (n *TailscaleNode) capturingLogf() func(format string, args ...any) {
	return func(format string, args ...any) {
		msg := fmt.Sprintf(format, args...)
		log.Printf(format, args...)

		if msg == "" {
			return
		}

		u := extractURL(msg)
		if u == "" {
			return
		}

		n.mu.Lock()
		if n.status.LoginURL == "" {
			n.status.LoginURL = u
		}
		n.mu.Unlock()
	}
}

func extractURL(msg string) string {
	const prefix = "https://"
	i := strings.Index(msg, prefix)
	if i < 0 {
		return ""
	}
	rest := msg[i+len(prefix):]
	end := strings.IndexAny(rest, " \t\n\r")
	if end < 0 {
		end = len(rest)
	}
	if end == 0 {
		return ""
	}
	url := prefix + rest[:end]

	// Auth URLs always have a path component (e.g. /a/xxx).
	// Control server URLs have no path (e.g. https://controlplane.tailscale.com:).
	if !strings.Contains(url[len(prefix):], "/") {
		return ""
	}

	return url
}

func (n *TailscaleNode) monitorConnection(ctx context.Context) {
	ticker := time.NewTicker(3 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			n.pollStatus()
		}
	}
}

func (n *TailscaleNode) pollStatus() {
	n.mu.Lock()
	defer n.mu.Unlock()

	if n.server == nil {
		return
	}

	ip, _ := n.server.TailscaleIPs()
	domains := n.server.CertDomains()

	if !ip.IsUnspecified() && ip.IsValid() {
		n.status.TailscaleIPs = []string{ip.String()}
		n.status.Connected = true
		n.status.AuthNeeded = false
		n.status.Error = ""
	} else {
		n.status.TailscaleIPs = nil
		n.status.Connected = false
	}

	n.status.Online = n.status.Connected

	if n.status.LoginURL != "" && !n.status.Connected {
		n.status.AuthNeeded = true
		n.status.AuthMethod = "interactive"
	}

	if len(domains) > 0 {
		n.status.Hostname = domains[0]
	} else if n.hostname != "" {
		n.status.Hostname = n.hostname
	}
}
