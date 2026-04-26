package core

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
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

func (n *NginxManager) Start() error {
	return exec.Command("nginx", "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf")).Start()
}

func (n *NginxManager) Stop() error {
	return exec.Command("nginx", "-s", "stop").Run()
}

func (n *NginxManager) Reload() error {
	return exec.Command("nginx", "-s", "reload").Run()
}

func (n *NginxManager) Test() error {
	out, err := exec.Command("nginx", "-t", "-c", filepath.Join(n.cfg.NginxDir, "nginx.conf")).CombinedOutput()
	if err != nil {
		return fmt.Errorf("nginx config test failed: %s", out)
	}
	return nil
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
