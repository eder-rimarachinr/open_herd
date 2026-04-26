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
	home, _ := os.UserHomeDir()
	base := filepath.Join(home, ".phpenv")

	return &Config{
		BaseDir:     base,
		NginxDir:    filepath.Join(base, "nginx"),
		PHPDir:      filepath.Join(base, "php"),
		CertsDir:    filepath.Join(base, "certs"),
		LogsDir:     filepath.Join(base, "logs"),
		SitesDir:    filepath.Join(base, "nginx", "sites"),
		APIAddr:     "127.0.0.1:7878",
		HTTPPort:    80,
		HTTPSPort:   443,
		ScannedDirs: []string{},
		DefaultPHP:  "",
		OS:          runtime.GOOS,
	}
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
	dirs := []string{c.BaseDir, c.NginxDir, c.PHPDir, c.CertsDir, c.LogsDir, c.SitesDir}
	for _, d := range dirs {
		if err := os.MkdirAll(d, 0755); err != nil {
			return err
		}
	}
	return nil
}
