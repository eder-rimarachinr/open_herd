package core

import (
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"text/template"
	"time"

	"github.com/open-herd/phpenv/daemon/platform"
)

const nginxSiteTemplate = `server {
    listen {{ .HTTPPort }};
    server_name {{ .Domain }};
    root {{ .DocumentRoot }};
    index {{ if .HasPHP }}index.php {{ end }}index.html index.htm;

    access_log {{ .LogsDir }}/{{ .Domain }}-access.log;
    error_log  {{ .LogsDir }}/{{ .Domain }}-error.log;

    location / {
        {{ if eq .ProjectType "spa" -}}
        try_files $uri $uri/ /index.html;
        {{- else if eq .ProjectType "static" -}}
        try_files $uri $uri/ =404;
        {{- else -}}
        try_files $uri $uri/ /index.php?$query_string;
        {{ if eq .ProjectType "generic" }}autoindex on;{{ end }}
        {{- end }}
    }
{{ if .SubdirAlias }}
    location /{{ .DirName }}/ {
        alias {{ .DocumentRoot }}/;
    }
{{ end -}}
{{ if .HasPHP }}
    location ~ \.php$ {
        fastcgi_pass {{ .FastCGIAddr }};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;
        include fastcgi_params;
    }
{{ end }}
    location ~ /\.(?!well-known).* {
        deny all;
    }
}
{{ if .SSLEnabled }}
server {
    listen {{ .HTTPSPort }} ssl;
    server_name {{ .Domain }};
    root {{ .DocumentRoot }};
    index {{ if .HasPHP }}index.php {{ end }}index.html index.htm;

    ssl_certificate     {{ .CertsDir }}/{{ .Domain }}.pem;
    ssl_certificate_key {{ .CertsDir }}/{{ .Domain }}-key.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    access_log {{ .LogsDir }}/{{ .Domain }}-ssl-access.log;
    error_log  {{ .LogsDir }}/{{ .Domain }}-ssl-error.log;

    location / {
        {{ if eq .ProjectType "spa" -}}
        try_files $uri $uri/ /index.html;
        {{- else if eq .ProjectType "static" -}}
        try_files $uri $uri/ =404;
        {{- else -}}
        try_files $uri $uri/ /index.php?$query_string;
        {{ if eq .ProjectType "generic" }}autoindex on;{{ end }}
        {{- end }}
    }
{{ if .SubdirAlias }}
    location /{{ .DirName }}/ {
        alias {{ .DocumentRoot }}/;
    }
{{ end -}}
{{ if .HasPHP }}
    location ~ \.php$ {
        fastcgi_pass {{ .FastCGIAddr }};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;
        include fastcgi_params;
    }
{{ end }}
    location ~ /\.(?!well-known).* {
        deny all;
    }
}
{{ end }}`

const nginxMainTemplate = `worker_processes auto;
pid {{ .PidFile }};
error_log {{ .LogsDir }}/nginx-error.log;

events {
    worker_connections 1024;
}

http {
    include       {{ .MimeTypes }};
    default_type  application/octet-stream;
    sendfile      on;
    keepalive_timeout 65;

    include {{ .SitesDir }}/*.conf;
}
`

type nginxSiteData struct {
	Domain       string
	DocumentRoot string
	DirName      string
	SubdirAlias  bool
	HasPHP       bool
	FastCGIAddr  string
	HTTPPort     int
	HTTPSPort    int
	SSLEnabled   bool
	CertsDir     string
	LogsDir      string
	ProjectType  string
}

type nginxMainData struct {
	PidFile   string
	LogsDir   string
	SitesDir  string
	MimeTypes string
}

type NginxManager struct {
	cfg  *Config
	plat platform.Platform
}

func NewNginxManager(cfg *Config, plat platform.Platform) *NginxManager {
	return &NginxManager{cfg: cfg, plat: plat}
}

// nginxPath converts backslashes to forward slashes for use inside nginx.conf.
// nginx's config parser treats backslash as an escape character on all platforms.
func nginxPath(p string) string {
	return strings.ReplaceAll(p, `\`, `/`)
}

// nginxConf returns the absolute path to our managed nginx.conf.
func (n *NginxManager) nginxConf() string {
	return filepath.Join(n.cfg.NginxDir, "nginx.conf")
}

// mimeTypesPath returns the correct mime.types path for the current OS.
// On Windows it is relative (extracted next to nginx.exe).
// On Linux it uses the system path from the nginx package.
func (n *NginxManager) mimeTypesPath() string {
	if runtime.GOOS == "windows" {
		return "mime.types"
	}
	for _, p := range []string{"/etc/nginx/mime.types", "/usr/local/etc/nginx/mime.types"} {
		if fileExists(p) {
			return p
		}
	}
	return "/etc/nginx/mime.types"
}

func (n *NginxManager) GenerateMainConfig() error {
	tmpl := template.Must(template.New("main").Parse(nginxMainTemplate))
	f, err := os.Create(n.nginxConf())
	if err != nil {
		return err
	}
	defer f.Close()
	return tmpl.Execute(f, nginxMainData{
		PidFile:   nginxPath(filepath.Join(n.cfg.BaseDir, "nginx.pid")),
		LogsDir:   nginxPath(n.cfg.LogsDir),
		SitesDir:  nginxPath(n.cfg.SitesDir),
		MimeTypes: n.mimeTypesPath(),
	})
}

func (n *NginxManager) GenerateSiteConfig(site *Site, fastCGIAddr string) error {
	tmpl := template.Must(template.New("site").Parse(nginxSiteTemplate))

	docRoot := site.Path
	switch site.ProjectType {
	case ProjectTypeLaravel, ProjectTypeCI4:
		docRoot = filepath.Join(site.Path, "public")
	case ProjectTypeSPA:
		if fileExists(filepath.Join(site.Path, "dist", "index.html")) {
			docRoot = filepath.Join(site.Path, "dist")
		} else {
			docRoot = filepath.Join(site.Path, "build")
		}
	}

	hasPHP := site.ProjectType != ProjectTypeSPA && site.ProjectType != ProjectTypeStatic
	subdirAlias := site.ProjectType == ProjectTypeStatic || site.ProjectType == ProjectTypeGeneric

	configPath := filepath.Join(n.cfg.SitesDir, site.Domain+".conf")
	f, err := os.Create(configPath)
	if err != nil {
		return err
	}
	defer f.Close()

	return tmpl.Execute(f, nginxSiteData{
		Domain:       site.Domain,
		DocumentRoot: nginxPath(docRoot),
		DirName:      filepath.Base(site.Path),
		SubdirAlias:  subdirAlias,
		HasPHP:       hasPHP,
		FastCGIAddr:  fastCGIAddr,
		HTTPPort:     n.cfg.HTTPPort,
		HTTPSPort:    n.cfg.HTTPSPort,
		SSLEnabled:   site.SSLEnabled,
		CertsDir:     nginxPath(n.cfg.CertsDir),
		LogsDir:      nginxPath(n.cfg.LogsDir),
		ProjectType:  string(site.ProjectType),
	})
}

func (n *NginxManager) RemoveSiteConfig(domain string) error {
	return os.Remove(filepath.Join(n.cfg.SitesDir, domain+".conf"))
}

func (n *NginxManager) nginxBin() string {
	if runtime.GOOS == "windows" {
		return filepath.Join(n.cfg.NginxDir, "nginx.exe")
	}
	return "nginx"
}

// IsInstalled reports whether nginx is available.
// On Windows: checks that nginx.exe exists in our managed directory.
// On Linux: checks that nginx is on PATH (installed via package manager).
func (n *NginxManager) IsInstalled() bool {
	if runtime.GOOS == "windows" {
		return fileExists(n.nginxBin())
	}
	_, err := exec.LookPath("nginx")
	return err == nil
}

// Downloadable reports whether the daemon can download nginx automatically.
// Only true on Windows — Linux users must install via their package manager.
func (n *NginxManager) Downloadable() bool {
	return runtime.GOOS == "windows"
}

// Start starts nginx using our managed config.
// On Linux nginx needs root to bind port 80, so it runs via pkexec/sudo.
func (n *NginxManager) Start() error {
	if !n.IsInstalled() {
		if runtime.GOOS == "windows" {
			return fmt.Errorf("nginx is not installed — go to the Nginx tab to download it")
		}
		return fmt.Errorf("nginx not found in PATH — install it via your package manager (e.g. apt install nginx)")
	}

	// Check for port conflicts (common with Laravel Herd/XAMPP)
	if runtime.GOOS == "windows" {
		if err := checkPort(n.cfg.HTTPPort); err != nil {
			return fmt.Errorf("port %d is occupied. Please close Laravel Herd, XAMPP or other web servers", n.cfg.HTTPPort)
		}
		if err := checkPort(n.cfg.HTTPSPort); err != nil {
			return fmt.Errorf("port %d (SSL) is occupied. Please close Laravel Herd or other web servers", n.cfg.HTTPSPort)
		}
	}

	if runtime.GOOS == "linux" {
		return n.plat.ElevatedRun(n.nginxBin(), "-c", n.nginxConf())
	}
	cmd := exec.Command(n.nginxBin(), "-c", n.nginxConf())
	cmd.Dir = n.cfg.NginxDir
	if err := cmd.Start(); err != nil {
		return err
	}
	// Wait briefly so nginx can write its pid file and we can detect early failures.
	time.Sleep(400 * time.Millisecond)
	if !n.IsRunning() {
		if tail := n.ErrorLogTail(5); tail != "" {
			return fmt.Errorf("nginx failed to start: %s", tail)
		}
		return fmt.Errorf("nginx failed to start (check the error log)")
	}
	return nil
}

func checkPort(port int) error {
	l, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		return err
	}
	l.Close()
	return nil
}

// Stop sends the stop signal to the nginx master process.
// On Linux, elevation is required because the master runs as root.
func (n *NginxManager) Stop() error {
	if runtime.GOOS == "linux" {
		return n.plat.ElevatedRun(n.nginxBin(), "-s", "stop", "-c", n.nginxConf())
	}
	cmd := exec.Command(n.nginxBin(), "-s", "stop", "-c", n.nginxConf())
	cmd.Dir = n.cfg.NginxDir
	err := cmd.Run()

	// If official stop fails on Windows (e.g. OpenEvent failed), try manual kill.
	if err != nil && runtime.GOOS == "windows" {
		pidFile := filepath.Join(n.cfg.BaseDir, "nginx.pid")
		if data, readErr := os.ReadFile(pidFile); readErr == nil {
			var pid int
			fmt.Sscanf(string(data), "%d", &pid)
			if proc, findErr := os.FindProcess(pid); findErr == nil {
				_ = proc.Kill()
				_ = os.Remove(pidFile)
				return nil
			}
		}
	}
	return err
}

// Reload sends SIGHUP to the nginx master (graceful config reload).
// On Linux, elevation is required because the master runs as root.
func (n *NginxManager) Reload() error {
	if runtime.GOOS == "linux" {
		return n.plat.ElevatedRun(n.nginxBin(), "-s", "reload", "-c", n.nginxConf())
	}
	cmd := exec.Command(n.nginxBin(), "-s", "reload", "-c", n.nginxConf())
	cmd.Dir = n.cfg.NginxDir
	err := cmd.Run()

	// If official reload fails on Windows (e.g. OpenEvent failed), try a full restart.
	if err != nil && runtime.GOOS == "windows" {
		_ = n.Stop()
		return n.Start()
	}
	return err
}

// Test validates the nginx config without restarting.
// Does not require elevation — it only reads files.
func (n *NginxManager) Test() error {
	cmd := exec.Command(n.nginxBin(), "-t", "-c", n.nginxConf())
	cmd.Dir = n.cfg.NginxDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("nginx config test failed: %s", out)
	}
	return nil
}

// Version returns the nginx version string (e.g. "nginx/1.26.2"), or empty if not installed.
func (n *NginxManager) Version() string {
	if !n.IsInstalled() {
		return ""
	}
	out, _ := exec.Command(n.nginxBin(), "-v").CombinedOutput()
	line := strings.TrimSpace(string(out))
	if idx := strings.Index(line, "nginx/"); idx >= 0 {
		return line[idx:]
	}
	return line
}

// ErrorLogTail returns the last n lines of the nginx error log.
func (n *NginxManager) ErrorLogTail(lines int) string {
	logPath := filepath.Join(n.cfg.LogsDir, "nginx-error.log")
	data, err := os.ReadFile(logPath)
	if err != nil {
		return ""
	}
	all := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(all) > lines {
		all = all[len(all)-lines:]
	}
	return strings.Join(all, "\n")
}

// Download installs the nginx binary. Only supported on Windows.
func (n *NginxManager) Download() error {
	if runtime.GOOS != "windows" {
		return fmt.Errorf("nginx must be installed via your package manager on Linux (e.g. apt install nginx)")
	}
	return ensureNginxWindows(n.cfg.NginxDir)
}

func (n *NginxManager) IsRunning() bool {
	pidFile := filepath.Join(n.cfg.BaseDir, "nginx.pid")
	data, err := os.ReadFile(pidFile)
	if err != nil {
		return false
	}
	var pid int
	fmt.Sscanf(string(data), "%d", &pid)
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	return isProcessRunning(proc)
}
