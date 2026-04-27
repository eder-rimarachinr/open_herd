package core

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"text/template"
)

const nginxSiteTemplate = `server {
    listen {{ .HTTPPort }};
    server_name {{ .Domain }};
    root {{ .DocumentRoot }};
    index index.php index.html index.htm;

    access_log {{ .LogsDir }}/{{ .Domain }}-access.log;
    error_log  {{ .LogsDir }}/{{ .Domain }}-error.log;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        fastcgi_pass {{ .FastCGIAddr }};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;
        include fastcgi_params;
    }

    location ~ /\.(?!well-known).* {
        deny all;
    }
}
{{ if .SSLEnabled }}
server {
    listen {{ .HTTPSPort }} ssl;
    server_name {{ .Domain }};
    root {{ .DocumentRoot }};
    index index.php index.html index.htm;

    ssl_certificate     {{ .CertsDir }}/{{ .Domain }}.pem;
    ssl_certificate_key {{ .CertsDir }}/{{ .Domain }}-key.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    access_log {{ .LogsDir }}/{{ .Domain }}-ssl-access.log;
    error_log  {{ .LogsDir }}/{{ .Domain }}-ssl-error.log;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        fastcgi_pass {{ .FastCGIAddr }};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;
        include fastcgi_params;
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
    include       mime.types;
    default_type  application/octet-stream;
    sendfile      on;
    keepalive_timeout 65;

    include {{ .SitesDir }}/*.conf;
}
`

type nginxSiteData struct {
	Domain       string
	DocumentRoot string
	FastCGIAddr  string // "unix:/path" on Linux, "127.0.0.1:PORT" on Windows
	HTTPPort     int
	HTTPSPort    int
	SSLEnabled   bool
	CertsDir     string
	LogsDir      string
}

type nginxMainData struct {
	PidFile  string
	LogsDir  string
	SitesDir string
}

type NginxManager struct {
	cfg *Config
}

func NewNginxManager(cfg *Config) *NginxManager {
	return &NginxManager{cfg: cfg}
}

func (n *NginxManager) GenerateMainConfig() error {
	tmpl := template.Must(template.New("main").Parse(nginxMainTemplate))
	f, err := os.Create(filepath.Join(n.cfg.NginxDir, "nginx.conf"))
	if err != nil {
		return err
	}
	defer f.Close()
	return tmpl.Execute(f, nginxMainData{
		PidFile:  filepath.Join(n.cfg.BaseDir, "nginx.pid"),
		LogsDir:  n.cfg.LogsDir,
		SitesDir: n.cfg.SitesDir,
	})
}

func (n *NginxManager) GenerateSiteConfig(site *Site, fastCGIAddr string) error {
	tmpl := template.Must(template.New("site").Parse(nginxSiteTemplate))

	docRoot := site.Path
	if site.ProjectType == ProjectTypeLaravel || site.ProjectType == ProjectTypeCI4 {
		docRoot = filepath.Join(site.Path, "public")
	}

	configPath := filepath.Join(n.cfg.SitesDir, site.Domain+".conf")
	f, err := os.Create(configPath)
	if err != nil {
		return err
	}
	defer f.Close()

	return tmpl.Execute(f, nginxSiteData{
		Domain:       site.Domain,
		DocumentRoot: docRoot,
		FastCGIAddr:  fastCGIAddr,
		HTTPPort:     n.cfg.HTTPPort,
		HTTPSPort:    n.cfg.HTTPSPort,
		SSLEnabled:   site.SSLEnabled,
		CertsDir:     n.cfg.CertsDir,
		LogsDir:      n.cfg.LogsDir,
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

func (n *NginxManager) IsInstalled() bool {
	return fileExists(n.nginxBin())
}

func (n *NginxManager) Start() error {
	if !n.IsInstalled() {
		return fmt.Errorf("nginx is not installed — go to the Nginx tab to download it")
	}
	cmd := exec.Command(n.nginxBin(), "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf"))
	cmd.Dir = n.cfg.NginxDir
	return cmd.Start()
}

func (n *NginxManager) Stop() error {
	cmd := exec.Command(n.nginxBin(), "-s", "stop", "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf"))
	cmd.Dir = n.cfg.NginxDir
	return cmd.Run()
}

func (n *NginxManager) Reload() error {
	cmd := exec.Command(n.nginxBin(), "-s", "reload", "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf"))
	cmd.Dir = n.cfg.NginxDir
	return cmd.Run()
}

func (n *NginxManager) Test() error {
	cmd := exec.Command(n.nginxBin(), "-t", "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf"))
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
	// nginx writes version to stderr: "nginx version: nginx/1.26.2"
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

// Download installs the nginx binary (Windows only). Safe to call when already installed.
func (n *NginxManager) Download() error {
	if runtime.GOOS != "windows" {
		return fmt.Errorf("nginx must be installed via your package manager on Linux")
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
