package core

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
)

type Config struct {
	BaseDir     string   `json:"base_dir"`
	NginxDir    string   `json:"nginx_dir"`
	PHPDir      string   `json:"php_dir"`
	CertsDir    string   `json:"certs_dir"`
	LogsDir     string   `json:"logs_dir"`
	SitesDir    string   `json:"sites_dir"`
	APIAddr     string   `json:"api_addr"`
	HTTPPort    int      `json:"http_port"`
	HTTPSPort   int      `json:"https_port"`
	ScannedDirs    []string `json:"scanned_dirs"`
	DefaultPHP     string   `json:"default_php"`
	CustomPHPDirs  []string `json:"custom_php_dirs"`  // user-added PHP search paths
	OS             string   `json:"os"`
}

func DefaultConfig() *Config {
	// PHPENV_DATA_DIR lets the Tauri GUI tell the daemon where to store data.
	// Used for portable mode: GUI detects data/config.json next to itself and
	// passes the absolute path via this env var when spawning the sidecar.
	base := os.Getenv("PHPENV_DATA_DIR")

	// Portable fallback: data/config.json exists next to the daemon exe.
	// Covers go run / direct invocation outside Tauri.
	if base == "" {
		if exe, err := os.Executable(); err == nil {
			dataDir := filepath.Join(filepath.Dir(exe), "data")
			if _, err := os.Stat(filepath.Join(dataDir, "config.json")); err == nil {
				base = dataDir
			}
		}
	}

	// Default: ~/.phpenv — consistent across dev (go run) and production.
	if base == "" {
		home, _ := os.UserHomeDir()
		base = filepath.Join(home, ".phpenv")
	}

	cfg := &Config{
		BaseDir:     base,
		NginxDir:    filepath.Join(base, "nginx"),
		PHPDir:      filepath.Join(base, "php"),
		CertsDir:    filepath.Join(base, "certs"),
		LogsDir:     filepath.Join(base, "logs"),
		SitesDir:    filepath.Join(base, "nginx", "sites"),
		APIAddr:     DefaultAPIAddr,
		HTTPPort:    80,
		HTTPSPort:   443,
		ScannedDirs: []string{},
		DefaultPHP:  "",
		OS:          runtime.GOOS,
	}

	// Initialize with existing default paths so the user can see/manage them.
	cfg.CustomPHPDirs = cfg.GetDefaultPHPDirs()
	return cfg
}

func (c *Config) GetDefaultPHPDirs() []string {
	var paths []string
	if runtime.GOOS == "windows" {
		candidates := []string{
			`C:\xampp\php`,
			`C:\xampp64\php`,
			`C:\wamp\bin\php`,
			`C:\wamp64\bin\php`,
			`C:\laragon\bin\php`,
			`C:\php`,
			`C:\tools\php`,
		}
		for _, p := range candidates {
			if _, err := os.Stat(p); err == nil {
				paths = append(paths, p)
			}
		}
	} else {
		candidates := []string{"/usr/bin", "/usr/local/bin", "/usr/sbin"}
		for _, p := range candidates {
			if _, err := os.Stat(p); err == nil {
				paths = append(paths, p)
			}
		}
	}
	return paths
}

func LoadConfig() (*Config, error) {
	cfg := DefaultConfig()

	cfgPath := filepath.Join(cfg.BaseDir, "config.json")
	data, err := os.ReadFile(cfgPath)
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, nil
		}
		return nil, err
	}

	if err := json.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	return cfg, nil
}

func (c *Config) Save() error {
	if err := os.MkdirAll(c.BaseDir, 0755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(c.BaseDir, "config.json"), data, 0644)
}

func (c *Config) EnsureDirs() error {
	dirs := []string{
		c.BaseDir, c.NginxDir, c.PHPDir, c.CertsDir, c.LogsDir, c.SitesDir,
		// nginx looks for logs/error.log relative to its prefix (NginxDir) at startup.
		filepath.Join(c.NginxDir, "logs"),
		// nginx creates client_body_temp and friends inside $prefix/temp on Windows.
		filepath.Join(c.NginxDir, "temp"),
	}
	for _, d := range dirs {
		if err := os.MkdirAll(d, 0755); err != nil {
			return err
		}
	}
	return nil
}
