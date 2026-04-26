package core

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/google/uuid"
)

type ProjectType string

const (
	ProjectTypeLaravel   ProjectType = "laravel"
	ProjectTypeWordPress ProjectType = "wordpress"
	ProjectTypeGeneric   ProjectType = "generic"
)

type Site struct {
	ID          string      `json:"id"`
	Name        string      `json:"name"`
	Domain      string      `json:"domain"`
	Path        string      `json:"path"`
	PHPVersion  string      `json:"php_version"`
	ProjectType ProjectType `json:"project_type"`
	SSLEnabled  bool        `json:"ssl_enabled"`
	Active      bool        `json:"active"`
	CreatedAt   time.Time   `json:"created_at"`
	UpdatedAt   time.Time   `json:"updated_at"`
}

type SiteManager struct {
	cfg   *Config
	sites map[string]*Site
}

func NewSiteManager(cfg *Config) *SiteManager {
	return &SiteManager{cfg: cfg, sites: make(map[string]*Site)}
}

func (sm *SiteManager) Load() error {
	path := filepath.Join(sm.cfg.BaseDir, "sites.json")
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	var sites []*Site
	if err := json.Unmarshal(data, &sites); err != nil {
		return err
	}
	for _, s := range sites {
		sm.sites[s.ID] = s
	}
	return nil
}

func (sm *SiteManager) save() error {
	data, err := json.MarshalIndent(sm.List(), "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(sm.cfg.BaseDir, "sites.json"), data, 0644)
}

func (sm *SiteManager) List() []*Site {
	result := make([]*Site, 0, len(sm.sites))
	for _, s := range sm.sites {
		result = append(result, s)
	}
	return result
}

func (sm *SiteManager) Get(id string) (*Site, bool) {
	s, ok := sm.sites[id]
	return s, ok
}

func (sm *SiteManager) Add(site *Site) error {
	if site.ID == "" {
		site.ID = uuid.New().String()
	}
	now := time.Now()
	site.CreatedAt = now
	site.UpdatedAt = now
	sm.sites[site.ID] = site
	return sm.save()
}

func (sm *SiteManager) Update(site *Site) error {
	if _, ok := sm.sites[site.ID]; !ok {
		return ErrNotFound
	}
	site.UpdatedAt = time.Now()
	sm.sites[site.ID] = site
	return sm.save()
}

func (sm *SiteManager) Delete(id string) error {
	if _, ok := sm.sites[id]; !ok {
		return ErrNotFound
	}
	delete(sm.sites, id)
	return sm.save()
}

// Scan walks all ScannedDirs and returns directories not yet registered.
func (sm *SiteManager) Scan() ([]*Site, error) {
	var discovered []*Site

	for _, dir := range sm.cfg.ScannedDirs {
		entries, err := os.ReadDir(dir)
		if err != nil {
			continue
		}
		for _, entry := range entries {
			if !entry.IsDir() {
				continue
			}
			path := filepath.Join(dir, entry.Name())
			if sm.findByPath(path) != nil {
				continue
			}
			name := entry.Name()
			discovered = append(discovered, &Site{
				ID:          uuid.New().String(),
				Name:        name,
				Domain:      strings.ToLower(name) + ".test",
				Path:        path,
				PHPVersion:  sm.cfg.DefaultPHP,
				ProjectType: detectProjectType(path),
				Active:      true,
			})
		}
	}

	return discovered, nil
}

func (sm *SiteManager) findByPath(path string) *Site {
	for _, s := range sm.sites {
		if s.Path == path {
			return s
		}
	}
	return nil
}

func detectProjectType(path string) ProjectType {
	if fileExists(filepath.Join(path, "artisan")) {
		return ProjectTypeLaravel
	}
	if fileExists(filepath.Join(path, "wp-config.php")) || fileExists(filepath.Join(path, "wp-login.php")) {
		return ProjectTypeWordPress
	}
	return ProjectTypeGeneric
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
