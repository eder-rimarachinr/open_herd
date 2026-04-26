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
	"time"
)

// Compiler variants to try, in order of preference.
// windows.php.net uses vs16 for PHP 8.x and vc15 for PHP 7.x.
var compilerOrder = map[string][]string{
	"8.4": {"vs17", "vs16"},
	"8.3": {"vs16", "vs17"},
	"8.2": {"vs16"},
	"8.1": {"vs16"},
	"8.0": {"vs16"},
	"7.4": {"vc15"},
}

func (p *PHPManager) runInstall(major, version string, prog *InstallProgress) error {
	destDir := filepath.Join(p.cfg.PHPDir, major)
	if err := os.MkdirAll(destDir, 0755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}

	// ── Resolve download URL ──────────────────────────────────────────────────

	prog.set(InstallStateDownloading, "Locating PHP "+version+" on windows.php.net…", 3)

	downloadURL, err := resolveDownloadURL(major, version)
	if err != nil {
		return err
	}

	// ── Download ──────────────────────────────────────────────────────────────

	prog.set(InstallStateDownloading, "Downloading PHP "+version+"…", 8)

	tmpFile, err := os.CreateTemp("", "phpenv-php-*.zip")
	if err != nil {
		return err
	}
	defer os.Remove(tmpFile.Name())
	defer tmpFile.Close()

	resp, err := http.Get(downloadURL) //nolint:gosec
	if err != nil {
		return fmt.Errorf("download failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf(
			"download failed (HTTP %d)\nURL: %s\n\nTip: check that version %s is published at windows.php.net/download",
			resp.StatusCode, downloadURL, version,
		)
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
				pct := 8 + int(downloaded*60/total)
				prog.set(InstallStateDownloading,
					fmt.Sprintf("Downloading… %.1f / %.1f MB", mb(downloaded), mb(total)),
					pct,
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

	prog.set(InstallStateExtracting, "Extracting…", 72)
	if err := extractZip(tmpFile.Name(), destDir); err != nil {
		return fmt.Errorf("extract: %w", err)
	}

	// ── Configure php.ini ────────────────────────────────────────────────────

	prog.set(InstallStateConfiguring, "Configuring php.ini…", 92)
	if err := configureINI(destDir); err != nil {
		return fmt.Errorf("php.ini: %w", err)
	}

	return nil
}

// resolveDownloadURL tries each compiler variant (HEAD request) until one returns 200.
// It checks both the main releases folder and the archives folder.
func resolveDownloadURL(major, version string) (string, error) {
	compilers, ok := compilerOrder[major]
	if !ok {
		compilers = []string{"vs16", "vs17", "vc15"}
	}

	client := &http.Client{Timeout: 10 * time.Second}
	var tried []string

	for _, vs := range compilers {
		fileName := fmt.Sprintf("php-%s-nts-Win32-%s-x64.zip", version, vs)
		
		// 1. Try main releases folder
		url := "https://windows.php.net/downloads/releases/" + fileName
		tried = append(tried, url)
		if urlExists(client, url) {
			return url, nil
		}

		// 2. Try archives folder (where older patches go)
		archiveURL := "https://windows.php.net/downloads/releases/archives/" + fileName
		tried = append(tried, archiveURL)
		if urlExists(client, archiveURL) {
			return archiveURL, nil
		}
	}

	return "", fmt.Errorf(
		"PHP %s not found on windows.php.net.\n\nURLs tried:\n  %s\n\nTip: check that version %s is published at windows.php.net/download",
		version,
		strings.Join(tried, "\n  "),
		version,
	)
}

func urlExists(client *http.Client, url string) bool {
	req, _ := http.NewRequest(http.MethodHead, url, nil)
	resp, err := client.Do(req)
	if err != nil {
		return false
	}
	resp.Body.Close()
	return resp.StatusCode == http.StatusOK
}

func configureINI(destDir string) error {
	iniSrc := filepath.Join(destDir, "php.ini-production")
	iniDst := filepath.Join(destDir, "php.ini")

	if !fileExists(iniSrc) || fileExists(iniDst) {
		return nil
	}

	data, err := os.ReadFile(iniSrc)
	if err != nil {
		return err
	}

	content := string(data)
	for _, ext := range []string{"curl", "mbstring", "openssl", "pdo_mysql", "pdo_sqlite", "fileinfo", "intl", "zip"} {
		content = strings.ReplaceAll(content, ";extension="+ext, "extension="+ext)
	}
	content = strings.ReplaceAll(content, ";extension_dir = \"ext\"", "extension_dir = \"ext\"")

	return os.WriteFile(iniDst, []byte(content), 0644)
}

func extractZip(zipPath, destDir string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()

	prefix := filepath.Clean(destDir) + string(os.PathSeparator)

	for _, f := range r.File {
		outPath := filepath.Join(destDir, f.Name)
		if !strings.HasPrefix(filepath.Clean(outPath), prefix) {
			continue // zip-slip guard
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
