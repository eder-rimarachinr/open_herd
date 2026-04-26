//go:build windows

package core

import (
	"archive/zip"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

// vsVersion maps PHP major versions to their Windows compiler tag.
var vsVersion = map[string]string{
	"8.4": "vs17",
	"8.3": "vs17",
	"8.2": "vs16",
	"8.1": "vs16",
	"8.0": "vs16",
	"7.4": "vc15",
}

func (p *PHPManager) runInstall(major, version string, prog *InstallProgress) error {
	vs, ok := vsVersion[major]
	if !ok {
		vs = "vs16"
	}

	// e.g. https://windows.php.net/downloads/releases/php-8.3.20-nts-Win32-vs16-x64.zip
	url := fmt.Sprintf(
		"https://windows.php.net/downloads/releases/php-%s-nts-Win32-%s-x64.zip",
		version, vs,
	)

	destDir := filepath.Join(p.cfg.PHPDir, major)
	if err := os.MkdirAll(destDir, 0755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}

	// ── Download ──────────────────────────────────────────────────────────────

	prog.set(InstallStateDownloading, "Downloading PHP "+version+"…", 5)

	tmpFile, err := os.CreateTemp("", "phpenv-php-*.zip")
	if err != nil {
		return err
	}
	defer os.Remove(tmpFile.Name())
	defer tmpFile.Close()

	resp, err := http.Get(url) //nolint:gosec
	if err != nil {
		return fmt.Errorf("download: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download: HTTP %d from %s", resp.StatusCode, url)
	}

	total := resp.ContentLength
	var downloaded int64

	buf := make([]byte, 32*1024)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, writeErr := tmpFile.Write(buf[:n]); writeErr != nil {
				return fmt.Errorf("write tmp: %w", writeErr)
			}
			downloaded += int64(n)
			if total > 0 {
				pct := int(downloaded * 60 / total) // 5‥65%
				prog.set(InstallStateDownloading,
					fmt.Sprintf("Downloading %.1f / %.1f MB", mb(downloaded), mb(total)),
					5+pct,
				)
			}
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return fmt.Errorf("read: %w", readErr)
		}
	}

	// ── Extract ───────────────────────────────────────────────────────────────

	prog.set(InstallStateExtracting, "Extracting…", 70)

	if err := extractZip(tmpFile.Name(), destDir); err != nil {
		return fmt.Errorf("extract: %w", err)
	}

	// ── Configure php.ini ────────────────────────────────────────────────────

	prog.set(InstallStateConfiguring, "Configuring php.ini…", 90)

	iniProduction := filepath.Join(destDir, "php.ini-production")
	iniDest := filepath.Join(destDir, "php.ini")

	if fileExists(iniProduction) && !fileExists(iniDest) {
		src, err := os.ReadFile(iniProduction)
		if err != nil {
			return err
		}
		// Enable common extensions.
		content := string(src)
		for _, ext := range []string{"curl", "mbstring", "openssl", "pdo_mysql", "pdo_sqlite", "fileinfo", "intl"} {
			content = strings.ReplaceAll(content, ";extension="+ext, "extension="+ext)
		}
		// Point extension_dir to the bundled ext/ folder.
		content = strings.ReplaceAll(content,
			";extension_dir = \"ext\"",
			"extension_dir = \"ext\"",
		)
		if err := os.WriteFile(iniDest, []byte(content), 0644); err != nil {
			return err
		}
	}

	return nil
}

func extractZip(zipPath, destDir string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()

	for _, f := range r.File {
		outPath := filepath.Join(destDir, f.Name)

		// Guard against zip-slip.
		if !strings.HasPrefix(filepath.Clean(outPath), filepath.Clean(destDir)+string(os.PathSeparator)) {
			continue
		}

		if f.FileInfo().IsDir() {
			os.MkdirAll(outPath, 0755)
			continue
		}

		if err := os.MkdirAll(filepath.Dir(outPath), 0755); err != nil {
			return err
		}

		dst, err := os.Create(outPath)
		if err != nil {
			return err
		}

		src, err := f.Open()
		if err != nil {
			dst.Close()
			return err
		}

		_, copyErr := io.Copy(dst, src)
		src.Close()
		dst.Close()
		if copyErr != nil {
			return copyErr
		}
	}
	return nil
}

func mb(b int64) float64 { return float64(b) / 1024 / 1024 }
