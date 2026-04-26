package core

import (
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
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
