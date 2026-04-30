package handlers

import (
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/open-herd/phpenv/daemon/core"
)

func ListDatabases(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		instances := app.DB.List()
		type item struct {
			*core.DBInstance
			Running bool `json:"running"`
		}
		result := make([]item, 0, len(instances))
		for _, inst := range instances {
			result = append(result, item{inst, app.DB.IsRunning(inst)})
		}
		OK(w, result)
	}
}

func AddDatabase(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var inst core.DBInstance
		if err := json.NewDecoder(r.Body).Decode(&inst); err != nil {
			BadRequest(w, "invalid JSON")
			return
		}
		if inst.Name == "" {
			BadRequest(w, "name is required")
			return
		}
		if inst.Type == "" {
			BadRequest(w, "type is required (mysql, mariadb, postgres)")
			return
		}
		if err := app.DB.Add(&inst); err != nil {
			InternalError(w, err)
			return
		}
		Created(w, inst)
	}
}

func GetDatabase(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		inst, ok := app.DB.Get(chi.URLParam(r, "dbID"))
		if !ok {
			NotFound(w)
			return
		}
		type response struct {
			*core.DBInstance
			Running bool `json:"running"`
		}
		OK(w, response{inst, app.DB.IsRunning(inst)})
	}
}

func DeleteDatabase(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := app.DB.Delete(chi.URLParam(r, "dbID")); err != nil {
			InternalError(w, err)
			return
		}
		NoContent(w)
	}
}

func StartDatabase(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		inst, ok := app.DB.Get(chi.URLParam(r, "dbID"))
		if !ok {
			NotFound(w)
			return
		}
		if err := app.DB.Start(inst); err != nil {
			BadRequest(w, err.Error())
			return
		}
		OK(w, map[string]string{"status": "started"})
	}
}

func StopDatabase(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		inst, ok := app.DB.Get(chi.URLParam(r, "dbID"))
		if !ok {
			NotFound(w)
			return
		}
		if err := app.DB.Stop(inst); err != nil {
			BadRequest(w, err.Error())
			return
		}
		OK(w, map[string]string{"status": "stopped"})
	}
}

func DetectDatabases(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		found, err := app.DB.Detect()
		if err != nil {
			InternalError(w, err)
			return
		}
		for _, inst := range found {
			_ = app.DB.Add(inst)
		}
		// Return the full updated list with running status.
		instances := app.DB.List()
		type item struct {
			*core.DBInstance
			Running bool `json:"running"`
		}
		result := make([]item, 0, len(instances))
		for _, inst := range instances {
			result = append(result, item{inst, app.DB.IsRunning(inst)})
		}
		OK(w, result)
	}
}
