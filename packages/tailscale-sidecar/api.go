package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"
)

type StartRequest struct {
	AuthKey string `json:"authKey,omitempty"`
}

type AuthKeyRequest struct {
	AuthKey string `json:"authKey"`
}

type ListenRequest struct {
	LocalPort int `json:"localPort"`
}

type ListenResponse struct {
	Ok           bool     `json:"ok"`
	Port         int      `json:"port,omitempty"`
	TailscaleIPs []string `json:"tailscaleIPs,omitempty"`
	Error        string   `json:"error,omitempty"`
}

type StatusResponse struct {
	Ok           bool     `json:"ok"`
	Connected    bool     `json:"connected"`
	TailscaleIPs []string `json:"tailscaleIPs,omitempty"`
	Hostname     string   `json:"hostname,omitempty"`
	AuthNeeded   bool     `json:"authNeeded"`
	AuthMethod   string   `json:"authMethod,omitempty"`
	LoginURL     string   `json:"loginURL,omitempty"`
	Online       bool     `json:"online"`
	Error        string   `json:"error,omitempty"`
}

func startAPIServer(node *TailscaleNode, apiPort int) error {
	mux := http.NewServeMux()

	mux.HandleFunc("/api/status", func(w http.ResponseWriter, r *http.Request) {
		status := node.Status()
		resp := StatusResponse{
			Ok:           true,
			Connected:    status.Connected,
			TailscaleIPs: status.TailscaleIPs,
			Hostname:     status.Hostname,
			AuthNeeded:   status.AuthNeeded,
			AuthMethod:   status.AuthMethod,
			LoginURL:     status.LoginURL,
			Online:       status.Online,
			Error:        status.Error,
		}
		writeJSON(w, http.StatusOK, resp)
	})

	mux.HandleFunc("/api/start", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, map[string]any{"ok": false, "error": "method not allowed"})
			return
		}

		var req StartRequest
		if r.Body != nil {
			json.NewDecoder(r.Body).Decode(&req)
		}

		if req.AuthKey != "" {
			node.SetAuthKey(context.Background(), req.AuthKey)
		}

		ctx, cancel := context.WithCancel(context.Background())
		defer cancel()

		if err := node.Up(ctx); err != nil {
			writeJSON(w, http.StatusInternalServerError, map[string]any{"ok": false, "error": err.Error()})
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "message": "tailscale started"})
	})

	mux.HandleFunc("/api/stop", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, map[string]any{"ok": false, "error": "method not allowed"})
			return
		}

		if err := node.Down(); err != nil {
			writeJSON(w, http.StatusInternalServerError, map[string]any{"ok": false, "error": err.Error()})
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "message": "tailscale stopped"})
	})

	mux.HandleFunc("/api/auth-key", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			writeJSON(w, http.StatusMethodNotAllowed, map[string]any{"ok": false, "error": "method not allowed"})
			return
		}

		var req AuthKeyRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeJSON(w, http.StatusBadRequest, map[string]any{"ok": false, "error": "invalid request body"})
			return
		}

		if req.AuthKey == "" {
			writeJSON(w, http.StatusBadRequest, map[string]any{"ok": false, "error": "authKey is required"})
			return
		}

		if err := node.SetAuthKey(context.Background(), req.AuthKey); err != nil {
			writeJSON(w, http.StatusInternalServerError, map[string]any{"ok": false, "error": err.Error()})
			return
		}

		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "message": "auth key set, reconnecting"})
	})

	mux.HandleFunc("/api/login-url", func(w http.ResponseWriter, r *http.Request) {
		url, err := node.GetLoginURL()
		if err != nil {
			writeJSON(w, http.StatusNotFound, map[string]any{"ok": false, "error": err.Error()})
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "url": url})
	})

	mux.HandleFunc("/api/listen", func(w http.ResponseWriter, r *http.Request) {
		switch r.Method {
		case http.MethodPost:
			var req ListenRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				writeJSON(w, http.StatusBadRequest, ListenResponse{Ok: false, Error: err.Error()})
				return
			}
			if err := node.StartForwarder(context.Background(), req.LocalPort, "127.0.0.1", req.LocalPort); err != nil {
				writeJSON(w, http.StatusInternalServerError, ListenResponse{Ok: false, Error: err.Error()})
				return
			}
			ips := node.Status().TailscaleIPs
			writeJSON(w, http.StatusOK, ListenResponse{Ok: true, Port: req.LocalPort, TailscaleIPs: ips})

		case http.MethodDelete:
			node.StopForwarder()
			writeJSON(w, http.StatusOK, map[string]any{"ok": true})

		default:
			writeJSON(w, http.StatusMethodNotAllowed, map[string]any{"ok": false, "error": "method not allowed"})
		}
	})

	addr := fmt.Sprintf("127.0.0.1:%d", apiPort)
	listener, err := net.Listen("tcp", addr)
	if err != nil {
		return fmt.Errorf("api listen: %w", err)
	}

	actualPort := listener.Addr().(*net.TCPAddr).Port
	fmt.Printf("EC_SIDECAR_PORT=%d\n", actualPort)

	server := &http.Server{
		Handler: mux,
	}

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigCh
		log.Println("shutting down sidecar")
		node.Down()
		server.Close()
	}()

	log.Printf("sidecar API listening on 127.0.0.1:%d", actualPort)
	if err := server.Serve(listener); err != nil && err != http.ErrServerClosed {
		return fmt.Errorf("api server: %w", err)
	}

	return nil
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(v)
}
