package core

import (
	"log/slog"

	"github.com/open-herd/phpenv/daemon/platform"
)

// ServiceStatus summarises what is running right now.
type ServiceStatus struct {
	Nginx      bool                    `json:"nginx"`
	PHPVersions []PHPRunningStatus     `json:"php_versions"`
	AllRunning bool                    `json:"all_running"`
}

type PHPRunningStatus struct {
	Major   string `json:"major"`
	Version string `json:"version"`
	Running bool   `json:"running"`
}

// App is the central application object passed to all API handlers.
type App struct {
	Config  *Config
	Plat    platform.Platform
	Sites   SiteStore
	PHP     PHPRuntime
	Nginx   NginxController
	DNS     DNSController
	SSL     CertManager
	DB      DBController
	Watcher *DirWatcher
}

func NewApp(cfg *Config, plat platform.Platform) *App {
	app := &App{
		Config: cfg,
		Plat:   plat,
		Sites:  NewSiteManager(cfg),
		PHP:    NewPHPManager(cfg),
		Nginx:  NewNginxManager(cfg, plat),
		DNS:    NewDNSManager(cfg, plat),
		SSL:    NewSSLManager(cfg, plat),
		DB:     NewDBManager(cfg),
	}
	app.Watcher = NewDirWatcher(app)
	return app
}

func (a *App) Initialize() error {
	if err := a.Config.EnsureDirs(); err != nil {
		return err
	}
	if err := a.Sites.Load(); err != nil {
		return err
	}
	if err := a.DB.Load(); err != nil {
		slog.Warn("databases load failed", "err", err)
	}
	if err := a.PHP.Detect(); err != nil {
		slog.Warn("PHP detection failed", "err", err)
	}
	if err := a.EnsureTools(); err != nil {
		slog.Warn("tool bootstrap failed", "err", err)
	}
	if err := a.Nginx.GenerateMainConfig(); err != nil {
		slog.Warn("nginx main config generation failed", "err", err)
	}
	if err := a.Watcher.Start(a.Config.ScannedDirs); err != nil {
		slog.Warn("dirwatcher: failed to start", "err", err)
	}
	return nil
}

// StartServices starts PHP-FPM for all detected versions, generates nginx
// site configs, then starts nginx. DNS entries are registered for active sites.
func (a *App) StartServices() error {
	activeMajor := a.Config.DefaultPHP
	
	// If no default is set or it's invalid, pick the first installed version
	if activeMajor == "" {
		for _, v := range a.PHP.GetVersions() {
			activeMajor = v.Major
			a.Config.DefaultPHP = activeMajor // Update in memory
			break
		}
	}

	// 1. Start ONLY the active PHP-FPM version, and stop the others.
	for _, v := range a.PHP.GetVersions() {
		if v.Major == activeMajor {
			if !v.Running {
				if err := a.PHP.StartFPM(v.Major); err != nil {
					slog.Error("php-fpm start failed", "version", v.Major, "err", err)
				}
			}
		} else {
			if v.Running {
				_ = a.PHP.StopFPM(v.Major)
			}
		}
	}

	// 2. Generate per-site nginx configs.
	for _, site := range a.Sites.List() {
		if !site.Active {
			continue
		}
		phpV, ok := a.PHP.GetVersion(site.PHPVersion)
		if !ok {
			// Fall back to any running version.
			for _, v := range a.PHP.GetVersions() {
				if v.Running {
					phpV = v
					ok = true
					break
				}
			}
		}
		if ok {
			if err := a.Nginx.GenerateSiteConfig(site, phpV.FastCGIAddr); err != nil {
				slog.Error("nginx site config failed", "domain", site.Domain, "err", err)
			}
		}
		_ = a.DNS.AddSite(site.Domain)
	}

	// 3. Start or reload nginx.
	if a.Nginx.IsRunning() {
		if err := a.Nginx.Reload(); err != nil {
			slog.Error("nginx reload failed", "err", err)
		}
	} else {
		if err := a.Nginx.Start(); err != nil {
			return err
		}
	}
	return nil
}

// StopServices stops nginx and all running PHP-FPM processes.
// The daemon itself keeps running so the API stays available.
func (a *App) StopServices() {
	if a.Nginx.IsRunning() {
		if err := a.Nginx.Stop(); err != nil {
			slog.Error("nginx stop failed", "err", err)
		}
	}
	for _, v := range a.PHP.GetVersions() {
		if v.Running {
			if err := a.PHP.StopFPM(v.Major); err != nil {
				slog.Error("php-fpm stop failed", "version", v.Major, "err", err)
			}
		}
	}
}

// ServiceStatus returns a snapshot of what is currently running.
func (a *App) ServiceStatus() ServiceStatus {
	versions := a.PHP.GetVersions()
	phpStatus := make([]PHPRunningStatus, 0, len(versions))
	allRunning := a.Nginx.IsRunning()

	for _, v := range versions {
		if v.Major == a.Config.DefaultPHP || a.Config.DefaultPHP == "" {
			phpStatus = append(phpStatus, PHPRunningStatus{
				Major:   v.Major,
				Version: v.Version,
				Running: v.Running,
			})
			if !v.Running {
				allRunning = false
			}
		}
	}

	return ServiceStatus{
		Nginx:       a.Nginx.IsRunning(),
		PHPVersions: phpStatus,
		AllRunning:  allRunning,
	}
}

func (a *App) Shutdown() {
	a.Watcher.Stop()
	a.StopServices()
}
