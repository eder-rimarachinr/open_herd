package handlers

import (
	"net/http"
	"runtime"

	"github.com/open-herd/phpenv/daemon/core"
)

func NginxStatus(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, map[string]bool{"running": app.Nginx.IsRunning()})
	}
}

type nginxInfoResponse struct {
	Installed    bool   `json:"installed"`
	Running      bool   `json:"running"`
	Version      string `json:"version"`
	BinaryPath   string `json:"binary_path"`
	ConfigValid  *bool  `json:"config_valid"`
	ConfigError  string `json:"config_error"`
	ErrorLog     string `json:"error_log"`
	Downloadable bool   `json:"downloadable"` // false on Linux — must use package manager
	OS           string `json:"os"`
}

func NginxInfo(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		installed := app.Nginx.IsInstalled()
		running := app.Nginx.IsRunning()

		info := nginxInfoResponse{
			Installed:    installed,
			Running:      running,
			Version:      app.Nginx.Version(),
			ErrorLog:     app.Nginx.ErrorLogTail(40),
			Downloadable: app.Nginx.Downloadable(),
			OS:           runtime.GOOS,
		}

		if installed {
			err := app.Nginx.Test()
			valid := err == nil
			info.ConfigValid = &valid
			if err != nil {
				info.ConfigError = err.Error()
			}
		}

		OK(w, info)
	}
}

func DownloadNginx(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		task := app.Nginx.StartDownloadTask()

		go func() {
			task.Set(core.TaskStateRunning, "Downloading nginx…")
			if err := app.Nginx.Download(); err != nil {
				task.Fail(err)
				return
			}
			task.Set(core.TaskStateDone, "Nginx "+app.Nginx.Version()+" installed")
		}()

		Accepted(w, map[string]string{"status": "pending"})
	}
}

func NginxDownloadProgress(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		task := app.Nginx.GetDownloadProgress()
		if task == nil {
			NotFound(w)
			return
		}
		OK(w, task)
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
