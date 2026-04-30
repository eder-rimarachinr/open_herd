package core

import (
	"archive/zip"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// EnsureTools checks for necessary binaries (mkcert, nginx) and downloads them if missing.
func (a *App) EnsureTools() error {
	binDir := filepath.Join(a.Config.BaseDir, "bin")
	if err := os.MkdirAll(binDir, 0755); err != nil {
		return err
	}

	// 1. mkcert
	if err := a.ensureMkcert(binDir); err != nil {
		slog.Warn("mkcert setup failed", "err", err)
	}

	// 2. nginx (Windows only — Linux uses system nginx)
	if runtime.GOOS == "windows" {
		if err := ensureNginxWindows(a.Config.NginxDir); err != nil {
			slog.Warn("nginx setup failed", "err", err)
		}
	}

	return nil
}

func (a *App) ensureMkcert(binDir string) error {
	binaryName := "mkcert"
	if runtime.GOOS == "windows" {
		binaryName = "mkcert.exe"
	}
	path := filepath.Join(binDir, binaryName)

	if _, err := os.Stat(path); err == nil {
		return nil // already exists
	}

	slog.Info("downloading mkcert")
	
	var url string
	if runtime.GOOS == "windows" {
		url = "https://github.com/FiloSottile/mkcert/releases/latest/download/mkcert-v1.4.4-windows-amd64.exe"
	} else if runtime.GOOS == "linux" {
		url = "https://github.com/FiloSottile/mkcert/releases/latest/download/mkcert-v1.4.4-linux-amd64"
	} else {
		return fmt.Errorf("unsupported OS for mkcert auto-download")
	}

	if err := downloadFile(path, url); err != nil {
		return err
	}

	if runtime.GOOS != "windows" {
		_ = os.Chmod(path, 0755)
	}

	slog.Info("mkcert installed, running -install", "path", path)
	return a.SSL.Install()
}

// nginxVersions lists candidates in preference order; the first one that
// returns HTTP 200 from nginx.org is used.
var nginxVersions = []string{"1.26.2", "1.26.1", "1.24.0"}

func ensureNginxWindows(nginxDir string) error {
	// All three files must be present; re-extract if any is missing.
	required := []string{"nginx.exe", "mime.types", "fastcgi_params"}
	allPresent := true
	for _, f := range required {
		if !fileExists(filepath.Join(nginxDir, f)) {
			allPresent = false
			break
		}
	}
	if allPresent {
		return nil
	}

	client := &http.Client{Timeout: toolDownloadTimeout}

	for _, version := range nginxVersions {
		url := fmt.Sprintf("https://nginx.org/download/nginx-%s.zip", version)
		slog.Debug("nginx download: trying", "url", url)

		tmp, err := os.CreateTemp("", "nginx-*.zip")
		if err != nil {
			return err
		}
		tmpName := tmp.Name()

		resp, err := client.Get(url) //nolint:gosec
		if err != nil {
			tmp.Close()
			os.Remove(tmpName)
			continue
		}
		if resp.StatusCode != http.StatusOK {
			resp.Body.Close()
			tmp.Close()
			os.Remove(tmpName)
			slog.Warn("nginx download: HTTP error, trying next", "url", url, "status", resp.StatusCode)
			continue
		}

		slog.Info("nginx download: starting", "url", url)
		_, copyErr := io.Copy(tmp, resp.Body)
		resp.Body.Close()
		tmp.Close()
		if copyErr != nil {
			os.Remove(tmpName)
			return fmt.Errorf("download nginx: %w", copyErr)
		}

		prefix := fmt.Sprintf("nginx-%s/", version)
		if err := extractNginxZip(tmpName, nginxDir, prefix); err != nil {
			os.Remove(tmpName)
			return err
		}
		os.Remove(tmpName)
		slog.Info("nginx installed", "version", version, "dir", nginxDir)
		return nil
	}

	return fmt.Errorf("could not download nginx: all versions failed")
}

// extractNginxZip extracts the files required to run nginx from the zip into destDir.
func extractNginxZip(zipPath, destDir, prefix string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()

	needed := map[string]string{
		prefix + "nginx.exe":           "nginx.exe",
		prefix + "conf/mime.types":     "mime.types",
		prefix + "conf/fastcgi_params": "fastcgi_params",
	}

	for _, f := range r.File {
		name := strings.ReplaceAll(f.Name, "\\", "/")
		dstName, ok := needed[name]
		if !ok {
			continue
		}
		dstPath := filepath.Join(destDir, dstName)
		if err := os.MkdirAll(filepath.Dir(dstPath), 0755); err != nil {
			return err
		}
		src, err := f.Open()
		if err != nil {
			return err
		}
		dst, err := os.Create(dstPath)
		if err != nil {
			src.Close()
			return err
		}
		_, copyErr := io.Copy(dst, src)
		src.Close()
		dst.Close()
		if copyErr != nil {
			return copyErr
		}
		slog.Debug("nginx extract: file", "name", dstName)
	}
	return nil
}

func downloadFile(filepath string, url string) error {
	resp, err := http.Get(url)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("bad status: %s", resp.Status)
	}

	out, err := os.Create(filepath)
	if err != nil {
		return err
	}
	defer out.Close()

	_, err = io.Copy(out, resp.Body)
	return err
}
