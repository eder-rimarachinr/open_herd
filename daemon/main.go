package main

import (
	"context"
	"io"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"
	"time"

	"github.com/open-herd/phpenv/daemon/api"
	"github.com/open-herd/phpenv/daemon/core"
	"github.com/open-herd/phpenv/daemon/platform"
)

func main() {
	cfg, _ := core.LoadConfig()

	// Setup logging to file and stdout immediately to capture startup issues
	if err := os.MkdirAll(cfg.LogsDir, 0755); err == nil {
		logPath := filepath.Join(cfg.LogsDir, "daemon.log")
		if f, err := os.OpenFile(logPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); err == nil {
			log.SetOutput(io.MultiWriter(os.Stdout, f))
		}
	}

	log.Printf("Daemon starting (OS: %s, BaseDir: %s)", runtime.GOOS, cfg.BaseDir)

	// On Windows, re-launch as Administrator if not already elevated.
	ensureElevated()

	plat := platform.New()
	app := core.NewApp(cfg, plat)

	if err := app.Initialize(); err != nil {
		log.Fatalf("init: %v", err)
	}

	// Auto-start services if there are already configured sites.
	if len(app.Sites.List()) > 0 {
		if err := app.StartServices(); err != nil {
			log.Printf("auto-start services: %v", err)
		}
	}

	srv := api.NewServer(cfg, app)

	go func() {
		log.Printf("phpenv daemon listening on http://%s", cfg.APIAddr)
		if err := srv.Start(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("server: %v", err)
		}
	}()

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("shutting down daemon...")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	_ = srv.Shutdown(ctx)
	app.Shutdown()
}
