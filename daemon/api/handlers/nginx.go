package handlers

import (
	"net/http"

	"github.com/open-herd/phpenv/daemon/core"
)

func NginxStatus(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, map[string]bool{"running": app.Nginx.IsRunning()})
	}
}

func StartNginx(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.Nginx.Start(); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "started"})
	}
}

func StopNginx(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.Nginx.Stop(); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "stopped"})
	}
}

func ReloadNginx(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.Nginx.Test(); err != nil {
			BadRequest(w, err.Error())
			return
		}
		if err := app.Nginx.Reload(); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "reloaded"})
	}
}
