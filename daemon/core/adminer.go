package core

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

const adminerDownloadURL = "https://github.com/vrana/adminer/releases/download/v4.8.1/adminer-4.8.1.php"
const AdminerPort = 8080

// adminerWrapper overrides Adminer's login() to allow empty passwords in local dev.
// Written as a plain string to avoid any multi-byte encoding issues in the PHP file.
const adminerWrapper = "<?php\nfunction adminer_object() {\n    class AdminerLocalDev extends Adminer {\n        function login($login, $password) { return true; }\n    }\n    return new AdminerLocalDev;\n}\nrequire __DIR__ . '/adminer-core.php';\n"

// AdminerStatus is returned by the /databases/adminer endpoint.
type AdminerStatus struct {
	Installed bool   `json:"installed"`
	URL       string `json:"url"`
	PHPReady  bool   `json:"php_ready"`
}

func (a *App) adminerDir() string {
	return filepath.Join(a.Config.BaseDir, "databases", "adminer")
}

func (a *App) adminerPHP() string {
	return filepath.Join(a.adminerDir(), "adminer-core.php")
}

func (a *App) adminerWrapper() string {
	return filepath.Join(a.adminerDir(), "adminer.php")
}

func (a *App) adminerConf() string {
	return filepath.Join(a.Config.NginxDir, "sites", "adminer.conf")
}

func (a *App) AdminerStatus() AdminerStatus {
	_, phpReady := a.firstRunningPHP()
	return AdminerStatus{
		Installed: fileExists(a.adminerPHP()) && fileExists(a.adminerWrapper()),
		URL:       fmt.Sprintf("http://127.0.0.1:%d", AdminerPort),
		PHPReady:  phpReady,
	}
}

// SetupAdminer downloads Adminer, writes a no-password wrapper, and generates the nginx vhost.
func (a *App) SetupAdminer(ctx context.Context) error {
	fastcgiAddr, ok := a.firstRunningPHP()
	if !ok {
		return fmt.Errorf("no running PHP version — start PHP first")
	}

	if err := os.MkdirAll(a.adminerDir(), 0755); err != nil {
		return err
	}

	// 1. Download Adminer core to adminer-core.php.
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, adminerDownloadURL, nil)
	if err != nil {
		return err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return fmt.Errorf("download Adminer: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download Adminer: HTTP %d", resp.StatusCode)
	}
	f, err := os.Create(a.adminerPHP())
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, resp.Body); err != nil {
		f.Close()
		return fmt.Errorf("write adminer-core.php: %w", err)
	}
	f.Close()

	// 2. Write wrapper that disables the empty-password restriction.
	if err := os.WriteFile(a.adminerWrapper(), []byte(adminerWrapper), 0644); err != nil {
		return fmt.Errorf("write adminer.php: %w", err)
	}

	// 3. Generate nginx vhost config.
	return a.writeAdminerNginxConfig(fastcgiAddr)
}

func (a *App) writeAdminerNginxConfig(fastcgiAddr string) error {
	// nginx on Windows needs forward slashes in paths.
	root := a.adminerDir()
	if runtime.GOOS == "windows" {
		root = strings.ReplaceAll(root, `\`, `/`)
	}

	cfg := fmt.Sprintf(`server {
    listen %d;
    server_name localhost 127.0.0.1;
    root %s;
    index adminer.php;

    location / {
        try_files $uri /adminer.php$is_args$args;
    }

    location ~ \.php$ {
        fastcgi_pass %s;
        fastcgi_index adminer.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_param PHP_VALUE "display_errors=On\nerror_reporting=-1";
        include fastcgi_params;
        fastcgi_read_timeout 300;
    }

    location ~ /\.(?!well-known).* { deny all; }
}
`, AdminerPort, root, fastcgiAddr)

	return os.WriteFile(a.adminerConf(), []byte(cfg), 0644)
}

func (a *App) firstRunningPHP() (fastcgiAddr string, ok bool) {
	for _, v := range a.PHP.GetVersions() {
		if v.Running {
			return v.FastCGIAddr, true
		}
	}
	return "", false
}
