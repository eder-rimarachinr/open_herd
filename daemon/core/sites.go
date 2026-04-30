package core

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
)

type ProjectType string

const (
	ProjectTypeLaravel   ProjectType = "laravel"
	ProjectTypeWordPress ProjectType = "wordpress"
	ProjectTypeCI4       ProjectType = "codeigniter4"
	ProjectTypeCI3       ProjectType = "codeigniter3"
	ProjectTypeSPA       ProjectType = "spa"
	ProjectTypeStatic    ProjectType = "static"
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
	mu    sync.RWMutex
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

func (sm *SiteManager) saveLocked() error {
	result := make([]*Site, 0, len(sm.sites))
	for _, s := range sm.sites {
		result = append(result, s)
	}
	data, err := json.MarshalIndent(result, "", "  ")
	if err != nil {
		return err
	}
	finalPath := filepath.Join(sm.cfg.BaseDir, "sites.json")
	// Write to a sibling temp file so os.Rename is a single syscall on the
	// same filesystem — the file is either fully written or not replaced at all.
	tmpPath := finalPath + ".tmp"
	if err := os.WriteFile(tmpPath, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmpPath, finalPath)
}

func (sm *SiteManager) List() []*Site {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	result := make([]*Site, 0, len(sm.sites))
	for _, s := range sm.sites {
		result = append(result, s)
	}
	return result
}

func (sm *SiteManager) Get(id string) (*Site, bool) {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	s, ok := sm.sites[id]
	return s, ok
}

func (sm *SiteManager) Add(site *Site) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	if site.ID == "" {
		site.ID = uuid.New().String()
	}
	now := time.Now()
	site.CreatedAt = now
	site.UpdatedAt = now
	sm.sites[site.ID] = site
	return sm.saveLocked()
}

func (sm *SiteManager) Update(site *Site) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	if _, ok := sm.sites[site.ID]; !ok {
		return ErrNotFound
	}
	site.UpdatedAt = time.Now()
	sm.sites[site.ID] = site
	return sm.saveLocked()
}

func (sm *SiteManager) Delete(id string) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	if _, ok := sm.sites[id]; !ok {
		return ErrNotFound
	}
	delete(sm.sites, id)
	return sm.saveLocked()
}

// Scan walks all ScannedDirs, persists any newly found sites, prunes deleted
// ones, and returns the full updated site list.
func (sm *SiteManager) Scan() ([]*Site, error) {
	sm.mu.Lock()
	// Prune registered sites that no longer exist on disk.
	for id, s := range sm.sites {
		if _, err := os.Stat(s.Path); os.IsNotExist(err) {
			delete(sm.sites, id)
		}
	}
	sm.mu.Unlock()

	now := time.Now()
	var added []*Site

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
			existing := sm.findByPath(path)
			if existing != nil {
				// Re-evaluate project type in case it changed or was detected wrong
				newType := detectProjectType(path)
				if existing.ProjectType != newType {
					sm.mu.Lock()
					existing.ProjectType = newType
					sm.mu.Unlock()
					// Signal that something changed and we might need to refresh configs
					added = append(added, existing) 
				}
				continue
			}
			name := entry.Name()
			s := &Site{
				ID:          uuid.New().String(),
				Name:        name,
				Domain:      strings.ToLower(name) + ".test",
				Path:        path,
				PHPVersion:  sm.cfg.DefaultPHP,
				ProjectType: detectProjectType(path),
				Active:      true,
				CreatedAt:   now,
				UpdatedAt:   now,
			}
			sm.mu.Lock()
			sm.sites[s.ID] = s
			sm.mu.Unlock()
			added = append(added, s)
		}
	}

	sm.mu.Lock()
	err := sm.saveLocked()
	sm.mu.Unlock()
	
	if err != nil {
		return nil, err
	}

	_ = added // caller can use List() for the full set
	return sm.List(), nil
}

func (sm *SiteManager) AddMultiple(sites []*Site) error {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	
	now := time.Now()
	for _, s := range sites {
		if s.ID == "" {
			s.ID = uuid.New().String()
		}
		s.CreatedAt = now
		s.UpdatedAt = now
		sm.sites[s.ID] = s
	}
	
	return sm.saveLocked()
}

func (sm *SiteManager) findByPath(path string) *Site {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	for _, s := range sm.sites {
		if s.Path == path {
			return s
		}
	}
	return nil
}

func DetectProjectType(path string) ProjectType {
	return detectProjectType(path)
}

func detectProjectType(path string) ProjectType {
	// Laravel: artisan + public/
	if fileExists(filepath.Join(path, "artisan")) && fileExists(filepath.Join(path, "public")) {
		return ProjectTypeLaravel
	}
	// CodeIgniter 4
	if fileExists(filepath.Join(path, "spark")) {
		return ProjectTypeCI4
	}
	// CodeIgniter 3
	if fileExists(filepath.Join(path, "application")) && fileExists(filepath.Join(path, "system")) && fileExists(filepath.Join(path, "index.php")) {
		return ProjectTypeCI3
	}
	// WordPress
	if fileExists(filepath.Join(path, "wp-config.php")) || fileExists(filepath.Join(path, "wp-login.php")) {
		return ProjectTypeWordPress
	}
	// Compiled SPA (React/Vue/Angular — Vite, CRA, Vue CLI, etc.)
	if fileExists(filepath.Join(path, "dist", "index.html")) || fileExists(filepath.Join(path, "build", "index.html")) {
		return ProjectTypeSPA
	}
	// Pure HTML
	if fileExists(filepath.Join(path, "index.html")) || fileExists(filepath.Join(path, "index.htm")) {
		return ProjectTypeStatic
	}
	return ProjectTypeGeneric
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
