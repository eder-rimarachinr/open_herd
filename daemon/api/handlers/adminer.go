package handlers

import (
	"context"
	"net/http"
	"time"

	"github.com/open-herd/phpenv/daemon/core"
)

func GetAdminerStatus(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.AdminerStatus())
	}
}

func SetupAdminer(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		ctx, cancel := context.WithTimeout(r.Context(), 30*time.Second)
		defer cancel()

		if err := app.SetupAdminer(ctx); err != nil {
			BadRequest(w, err.Error())
			return
		}

		// Reload nginx so the new vhost takes effect immediately.
		if app.Nginx.IsRunning() {
			_ = app.Nginx.Reload()
		}

		OK(w, app.AdminerStatus())
	}
}
