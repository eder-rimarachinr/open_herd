package handlers

import (
	"net/http"
	"os"
	"time"

	"github.com/open-herd/phpenv/daemon/core"
)

func ServicesStatus(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.ServiceStatus())
	}
}

func StartServices(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.StartServices(); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, app.ServiceStatus())
	}
}

func StopServices(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		app.StopServices()
		OK(w, app.ServiceStatus())
	}
}

// QuitDaemon stops all services then exits the process after a short delay
// so the HTTP response can be delivered first.
func QuitDaemon(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		app.StopServices()
		OK(w, map[string]string{"status": "quitting"})
		go func() {
			time.Sleep(300 * time.Millisecond)
			os.Exit(0)
		}()
	}
}
