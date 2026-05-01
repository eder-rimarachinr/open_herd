package core

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

type SiteInfo struct {
	AppName          string `json:"app_name"`
	AppEnv           string `json:"app_env"`
	AppDebug         bool   `json:"app_debug"`
	AppURL           string `json:"app_url"`
	AppTimezone      string `json:"app_timezone"`
	AppLocale        string `json:"app_locale"`
	FrameworkName    string `json:"framework_name"`
	FrameworkVersion string `json:"framework_version"`
	MaintenanceMode  bool   `json:"maintenance_mode"`
}

func ReadSiteInfo(site *Site) SiteInfo {
	info := SiteInfo{}
	readDotEnv(site.Path, &info)
	readComposerLock(site.Path, &info)
	info.MaintenanceMode = checkMaintenanceMode(site)
	return info
}

func readDotEnv(path string, info *SiteInfo) {
	f, err := os.Open(filepath.Join(path, ".env"))
	if err != nil {
		return
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			continue
		}
		key := strings.TrimSpace(parts[0])
		val := strings.Trim(strings.TrimSpace(parts[1]), `"'`)
		switch key {
		case "APP_NAME":
			info.AppName = val
		case "APP_ENV":
			info.AppEnv = val
		case "APP_DEBUG":
			info.AppDebug = strings.ToLower(val) == "true"
		case "APP_URL":
			info.AppURL = val
		case "APP_TIMEZONE":
			info.AppTimezone = val
		case "APP_LOCALE":
			info.AppLocale = val
		}
	}
}

type composerLock struct {
	Packages []struct {
		Name    string `json:"name"`
		Version string `json:"version"`
	} `json:"packages"`
}

var knownFrameworks = map[string]string{
	"laravel/framework":      "Laravel",
	"codeigniter4/framework": "CodeIgniter 4",
	"codeigniter/framework":  "CodeIgniter 3",
}

func readComposerLock(path string, info *SiteInfo) {
	data, err := os.ReadFile(filepath.Join(path, "composer.lock"))
	if err != nil {
		return
	}
	var lock composerLock
	if err := json.Unmarshal(data, &lock); err != nil {
		return
	}
	for _, pkg := range lock.Packages {
		if name, ok := knownFrameworks[pkg.Name]; ok {
			info.FrameworkName = name
			info.FrameworkVersion = strings.TrimPrefix(pkg.Version, "v")
			return
		}
	}
}

func checkMaintenanceMode(site *Site) bool {
	// Laravel
	if fileExists(filepath.Join(site.Path, "storage", "framework", "maintenance.php")) {
		return true
	}
	// CodeIgniter 4
	if fileExists(filepath.Join(site.Path, "writable", "cache", "maintenance.php")) {
		return true
	}
	return false
}
