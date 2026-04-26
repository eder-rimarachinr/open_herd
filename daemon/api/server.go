package api

import (
	"context"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	chimw "github.com/go-chi/chi/v5/middleware"
	"github.com/open-herd/phpenv/daemon/api/handlers"
	"github.com/open-herd/phpenv/daemon/core"
)

type Server struct {
	cfg     *core.Config
	app     *core.App
	httpSrv *http.Server
}

func NewServer(cfg *core.Config, app *core.App) *Server {
	return &Server{cfg: cfg, app: app}
}

func (s *Server) Start() error {
	r := chi.NewRouter()
	r.Use(chimw.Logger)
	r.Use(chimw.Recoverer)
	r.Use(corsMiddleware)

	r.Route("/api/v1", func(r chi.Router) {
		r.Get("/status", handlers.Status(s.app))

		r.Route("/sites", func(r chi.Router) {
			r.Get("/", handlers.ListSites(s.app))
			r.Post("/", handlers.CreateSite(s.app))
			r.Post("/scan", handlers.ScanSites(s.app))
			r.Route("/{siteID}", func(r chi.Router) {
				r.Get("/", handlers.GetSite(s.app))
				r.Put("/", handlers.UpdateSite(s.app))
				r.Delete("/", handlers.DeleteSite(s.app))
				r.Post("/ssl", handlers.EnableSSL(s.app))
				r.Delete("/ssl", handlers.DisableSSL(s.app))
			})
		})

		r.Route("/php", func(r chi.Router) {
			r.Get("/versions", handlers.ListPHPVersions(s.app))
			r.Get("/catalog", handlers.PHPCatalog(s.app))
			r.Post("/install", handlers.InstallPHP(s.app))
			r.Get("/install/{major}/progress", handlers.InstallPHPProgress(s.app))
			r.Post("/versions/{version}/start", handlers.StartPHPFPM(s.app))
			r.Post("/versions/{version}/stop", handlers.StopPHPFPM(s.app))
		})

		r.Route("/nginx", func(r chi.Router) {
			r.Get("/status", handlers.NginxStatus(s.app))
			r.Post("/start", handlers.StartNginx(s.app))
			r.Post("/stop", handlers.StopNginx(s.app))
			r.Post("/reload", handlers.ReloadNginx(s.app))
		})

		r.Route("/config", func(r chi.Router) {
			r.Get("/", handlers.GetConfig(s.app))
			r.Put("/", handlers.UpdateConfig(s.app))
		})

		r.Route("/services", func(r chi.Router) {
			r.Get("/status", handlers.ServicesStatus(s.app))
			r.Post("/start", handlers.StartServices(s.app))
			r.Post("/stop", handlers.StopServices(s.app))
		})

		r.Post("/daemon/quit", handlers.QuitDaemon(s.app))
	})

	s.httpSrv = &http.Server{
		Addr:         s.cfg.APIAddr,
		Handler:      r,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	return s.httpSrv.ListenAndServe()
}

func (s *Server) Shutdown(ctx context.Context) error {
	return s.httpSrv.Shutdown(ctx)
}
