package handlers

import (
	"encoding/json"
	"net/http"

	"github.com/open-herd/phpenv/daemon/core"
)

func GetConfig(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.Config)
	}
}

func UpdateConfig(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var patch core.Config
		if err := json.NewDecoder(r.Body).Decode(&patch); err != nil {
			BadRequest(w, "invalid JSON")
			return
		}

		// Only allow updating user-facing fields.
		scannedDirsChanged := false
		if len(patch.ScannedDirs) > 0 {
			app.Config.ScannedDirs = patch.ScannedDirs
			scannedDirsChanged = true
		}
		if patch.DefaultPHP != "" {
			app.Config.DefaultPHP = patch.DefaultPHP
		}
		// CustomPHPDirs can be set to an empty slice deliberately, so always apply.
		if patch.CustomPHPDirs != nil {
			app.Config.CustomPHPDirs = patch.CustomPHPDirs
		}

		if err := app.Config.Save(); err != nil {
			InternalError(w, err)
			return
		}

		// Restart watcher so new dirs are observed and removed dirs are dropped.
		if scannedDirsChanged {
			app.Watcher.Restart(app.Config.ScannedDirs)
		}

		OK(w, app.Config)
	}
}
