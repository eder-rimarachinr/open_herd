package handlers

import (
	"net/http"
	"runtime"
	"time"

	"github.com/open-herd/phpenv/daemon/core"
)

var startedAt = time.Now()

type statusResponse struct {
	Status      string         `json:"status"`
	Version     string         `json:"version"`
	Uptime      string         `json:"uptime"`
	OS          string         `json:"os"`
	PHPVersions []string       `json:"php_versions"`
	Nginx       nginxStatus    `json:"nginx"`
}

type nginxStatus struct {
	Running bool `json:"running"`
}

func Status(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		phpVersions := []string{}
		for _, v := range app.PHP.GetVersions() {
			phpVersions = append(phpVersions, v.Major)
		}

		OK(w, statusResponse{
			Status:      "ok",
			Version:     "0.1.0",
			Uptime:      time.Since(startedAt).Round(time.Second).String(),
			OS:          runtime.GOOS,
			PHPVersions: phpVersions,
			Nginx:       nginxStatus{Running: app.Nginx.IsRunning()},
		})
	}
}
