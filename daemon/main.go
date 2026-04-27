package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/open-herd/phpenv/daemon/api"
	"github.com/open-herd/phpenv/daemon/core"
	"github.com/open-herd/phpenv/daemon/platform"
)

func main() {
	// On Windows, re-launch as Administrator if not already elevated.
	// When spawned as a Tauri sidecar the parent's UAC token is inherited
	// so this is a no-op in that case.
	ensureElevated()

	cfg, err := core.LoadConfig()
	if err != nil {
		log.Fatalf("config: %v", err)
	}

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
