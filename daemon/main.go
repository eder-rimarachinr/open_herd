package main

import (
	"context"
	"io"
	"log/slog"
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

	// Build a writer that fans out to stdout and a log file.
	logWriter := io.Writer(os.Stdout)
	if err := os.MkdirAll(cfg.LogsDir, 0755); err == nil {
		logPath := filepath.Join(cfg.LogsDir, "daemon.log")
		if f, err := os.OpenFile(logPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644); err == nil {
			logWriter = io.MultiWriter(os.Stdout, f)
		}
	}

	// Structured logging via slog. Set PHPENV_LOG_LEVEL=debug for verbose output.
	logLevel := slog.LevelInfo
	if os.Getenv("PHPENV_LOG_LEVEL") == "debug" {
		logLevel = slog.LevelDebug
	}
	slog.SetDefault(slog.New(slog.NewTextHandler(logWriter, &slog.HandlerOptions{Level: logLevel})))

	slog.Info("daemon starting", "os", runtime.GOOS, "base_dir", cfg.BaseDir)

	// On Windows, re-launch as Administrator if not already elevated.
	ensureElevated()

	plat := platform.New()
	app := core.NewApp(cfg, plat)

	if err := app.Initialize(); err != nil {
		slog.Error("init failed", "err", err)
		os.Exit(1)
	}

	// Auto-start services if there are already configured sites.
	if len(app.Sites.List()) > 0 {
		if err := app.StartServices(); err != nil {
			slog.Warn("auto-start services failed", "err", err)
		}
	}

	srv := api.NewServer(cfg, app)

	go func() {
		slog.Info("daemon listening", "addr", cfg.APIAddr)
		if err := srv.Start(); err != nil && err != http.ErrServerClosed {
			slog.Error("HTTP server error", "err", err)
			os.Exit(1)
		}
	}()

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	slog.Info("shutting down")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	_ = srv.Shutdown(ctx)
	app.Shutdown()
}
