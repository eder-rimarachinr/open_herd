package handlers

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/open-herd/phpenv/daemon/core"
)

func ListSites(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		OK(w, app.Sites.List())
	}
}

func ScanSites(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		found, err := app.Sites.Scan()
		if err != nil {
			InternalError(w, err)
			return
		}
		OK(w, found)
	}
}

func GetSite(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		site, ok := app.Sites.Get(chi.URLParam(r, "siteID"))
		if !ok {
			NotFound(w)
			return
		}
		OK(w, site)
	}
}

type createSiteRequest struct {
	Name       string `json:"name"`
	Domain     string `json:"domain"`
	Path       string `json:"path"`
	PHPVersion string `json:"php_version"`
}

func CreateSite(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var req createSiteRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			BadRequest(w, "invalid JSON")
			return
		}
		if req.Path == "" {
			BadRequest(w, "path is required")
			return
		}

		site := &core.Site{
			Name:        req.Name,
			Domain:      req.Domain,
			Path:        req.Path,
			PHPVersion:  req.PHPVersion,
			ProjectType: core.ProjectTypeGeneric,
			Active:      true,
		}
		if site.Domain == "" {
			site.Domain = site.Name + ".test"
		}

		if err := app.Sites.Add(site); err != nil {
			InternalError(w, err)
			return
		}

		// Register DNS entry and regenerate nginx config.
		_ = app.DNS.AddSite(site.Domain)
		if phpVer, ok := app.PHP.GetVersion(site.PHPVersion); ok {
			_ = app.Nginx.GenerateSiteConfig(site, phpVer.FastCGIAddr)
			_ = app.Nginx.Reload()
		}

		Created(w, site)
	}
}

func UpdateSite(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		site, ok := app.Sites.Get(chi.URLParam(r, "siteID"))
		if !ok {
			NotFound(w)
			return
		}

		if err := json.NewDecoder(r.Body).Decode(site); err != nil {
			BadRequest(w, "invalid JSON")
			return
		}

		if err := app.Sites.Update(site); err != nil {
			InternalError(w, err)
			return
		}
		OK(w, site)
	}
}

func DeleteSite(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id := chi.URLParam(r, "siteID")
		site, ok := app.Sites.Get(id)
		if !ok {
			NotFound(w)
			return
		}

		_ = app.DNS.RemoveSite(site.Domain)
		_ = app.Nginx.RemoveSiteConfig(site.Domain)
		_ = app.Nginx.Reload()

		if err := app.Sites.Delete(id); err != nil {
			InternalError(w, err)
			return
		}
		NoContent(w)
	}
}

func EnableSSL(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		site, ok := app.Sites.Get(chi.URLParam(r, "siteID"))
		if !ok {
			NotFound(w)
			return
		}

		if err := app.SSL.IssueCert(site.Domain); err != nil {
			InternalError(w, err)
			return
		}

		site.SSLEnabled = true
		_ = app.Sites.Update(site)

		if phpVer, ok := app.PHP.GetVersion(site.PHPVersion); ok {
			_ = app.Nginx.GenerateSiteConfig(site, phpVer.FastCGIAddr)
			_ = app.Nginx.Reload()
		}

		OK(w, site)
	}
}

func DisableSSL(app *core.App) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		site, ok := app.Sites.Get(chi.URLParam(r, "siteID"))
		if !ok {
			NotFound(w)
			return
		}

		_ = app.SSL.RevokeCert(site.Domain)
		site.SSLEnabled = false
		_ = app.Sites.Update(site)

		if phpVer, ok := app.PHP.GetVersion(site.PHPVersion); ok {
			_ = app.Nginx.GenerateSiteConfig(site, phpVer.FastCGIAddr)
			_ = app.Nginx.Reload()
		}

		OK(w, site)
	}
}

// siteError maps domain errors to HTTP responses.
func siteError(w http.ResponseWriter, err error) {
	if errors.Is(err, core.ErrNotFound) {
		NotFound(w)
	} else {
		InternalError(w, err)
	}
}
