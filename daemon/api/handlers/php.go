package handlers

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/open-herd/phpenv/daemon/core"
)

func ListPHPVersions(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.PHP.GetVersions())
	}
}

// DetectPHP re-runs PHP detection without restarting the daemon.
// Useful after installing PHP outside of phpenv (XAMPP, manual install, etc.)
func DetectPHP(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.PHP.Detect(); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, app.PHP.GetCatalog())
	}
}

func PHPCatalog(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.PHP.GetCatalog())
	}
}

func InstallPHP(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var body struct {
			Major string `json:"major"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Major == "" {
			BadRequest(w, "major version required (e.g. \"8.3\")")
			return
		}
		if err := app.PHP.Install(body.Major); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "installing", "major": body.Major})
	}
}

func InstallPHPProgress(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		major := chi.URLParam(r, "major")
		prog := app.PHP.GetInstallProgress(major)
		if prog == nil {
			NotFound(w)
			return
		}
		OK(w, prog)
	}
}

func StartPHPFPM(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		version := chi.URLParam(r, "version")
		if err := app.PHP.StartFPM(version); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "started", "version": version})
	}
}

func StopPHPFPM(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		version := chi.URLParam(r, "version")
		if err := app.PHP.StopFPM(version); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, map[string]string{"status": "stopped", "version": version})
	}
}
