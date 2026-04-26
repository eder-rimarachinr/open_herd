package core

import (
	"archive/zip"
	"fmt"
	"io"
	"log"
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
		log.Printf("Warning: could not setup mkcert: %v", err)
	}

	// 2. nginx (Windows only — Linux uses system nginx)
	if runtime.GOOS == "windows" {
		if err := a.ensureNginxWindows(); err != nil {
			log.Printf("Warning: could not setup nginx: %v", err)
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

	log.Printf("mkcert not found, downloading...")
	
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

	log.Printf("mkcert installed to %s. Running mkcert -install...", path)
	return a.SSL.Install()
}

const nginxVersion = "1.26.3"

func (a *App) ensureNginxWindows() error {
	nginxBin := filepath.Join(a.Config.NginxDir, "nginx.exe")
	if fileExists(nginxBin) {
		return nil
	}

	log.Printf("nginx not found, downloading nginx %s...", nginxVersion)

	url := fmt.Sprintf("https://nginx.org/download/nginx-%s.zip", nginxVersion)
	tmp, err := os.CreateTemp("", "nginx-*.zip")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())

	resp, err := http.Get(url) //nolint:gosec
	if err != nil {
		return fmt.Errorf("download nginx: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download nginx: HTTP %d", resp.StatusCode)
	}
	if _, err := io.Copy(tmp, resp.Body); err != nil {
		tmp.Close()
		return err
	}
	tmp.Close()

	prefix := fmt.Sprintf("nginx-%s/", nginxVersion)
	return extractNginxZip(tmp.Name(), a.Config.NginxDir, prefix)
}

// extractNginxZip extracts nginx.exe and conf/mime.types from the nginx zip into destDir.
func extractNginxZip(zipPath, destDir, prefix string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()

	needed := map[string]string{
		prefix + "nginx.exe":      "nginx.exe",
		prefix + "conf/mime.types": "mime.types",
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
		log.Printf("nginx: extracted %s", dstName)
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
