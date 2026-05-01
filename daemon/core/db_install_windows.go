//go:build windows

package core

import (
	"archive/zip"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

const mariadbAPIURL = "https://downloads.mariadb.org/rest-api/mariadb/11.4/"
const dbInstallTimeout = 15 * time.Minute

type mariadbReleasesResp struct {
	Releases map[string]mariadbRelease `json:"releases"`
}

type mariadbRelease struct {
	Files []mariadbFile `json:"files"`
}

type mariadbFile struct {
	FileName    string `json:"file_name"`
	OS          string `json:"os"`
	CPU         string `json:"cpu"`
	DownloadURL string `json:"file_download_url"`
}

// InstallLocal downloads MariaDB 11.4 LTS, extracts it to the phpenv data directory,
// initialises the data directory, and updates the DBInstance with the binary paths.
func (m *DBManager) InstallLocal(id string) {
	inst, ok := m.Get(id)
	if !ok {
		return
	}
	task := m.GetInstallProgress(id)
	if task == nil {
		return
	}

	ctx, cancel := context.WithTimeout(context.Background(), dbInstallTimeout)
	defer cancel()

	installDir := filepath.Join(m.cfg.BaseDir, "databases", id)
	dataDir := filepath.Join(installDir, "data")

	// 1. Resolve download URL via MariaDB REST API.
	task.Set(TaskStateRunning, "Checking latest MariaDB 11.4 LTS version…")
	version, downloadURL, err := resolveMariaDBDownload(ctx)
	if err != nil {
		task.Fail(err)
		return
	}

	// 2. Download ZIP to temp file.
	task.Set(TaskStateRunning, fmt.Sprintf("Downloading MariaDB %s…", version))
	tmpFile, err := os.CreateTemp("", "phpenv-mariadb-*.zip")
	if err != nil {
		task.Fail(err)
		return
	}
	defer os.Remove(tmpFile.Name())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, downloadURL, nil)
	if err != nil {
		task.Fail(fmt.Errorf("download request: %w", err))
		return
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		task.Fail(fmt.Errorf("download: %w", err))
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		task.Fail(fmt.Errorf("download failed (HTTP %d)", resp.StatusCode))
		return
	}

	total := resp.ContentLength
	var downloaded int64
	buf := make([]byte, 32*1024)
	for {
		n, readErr := resp.Body.Read(buf)
		if n > 0 {
			if _, writeErr := tmpFile.Write(buf[:n]); writeErr != nil {
				task.Fail(fmt.Errorf("write temp: %w", writeErr))
				return
			}
			downloaded += int64(n)
			if total > 0 {
				task.Set(TaskStateRunning, fmt.Sprintf(
					"Downloading MariaDB %s… %.0f / %.0f MB",
					version, mb(downloaded), mb(total),
				))
			}
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			task.Fail(fmt.Errorf("read: %w", readErr))
			return
		}
	}
	tmpFile.Close()

	// 3. Extract ZIP, stripping the top-level directory (e.g. mariadb-11.4.4-winx64/).
	task.Set(TaskStateRunning, "Extracting…")
	if err := os.MkdirAll(installDir, 0755); err != nil {
		task.Fail(err)
		return
	}
	if err := extractZipStrip1(tmpFile.Name(), installDir); err != nil {
		task.Fail(fmt.Errorf("extract: %w", err))
		return
	}

	// 4. Initialize the MariaDB data directory (creates system tables, root user with no password).
	task.Set(TaskStateRunning, "Initializing database…")
	// Remove the data dir if a previous failed attempt left partial files.
	if _, err := os.Stat(filepath.Join(dataDir, "mysql")); os.IsNotExist(err) {
		os.RemoveAll(dataDir)
	}
	if err := os.MkdirAll(dataDir, 0755); err != nil {
		task.Fail(err)
		return
	}
	binDir := filepath.Join(installDir, "bin")
	if err := runDBInit(ctx, binDir, dataDir); err != nil {
		// Attach the MariaDB error log if it exists for a better message.
		logMsg := ""
		if logData, lerr := readFirstErrLog(dataDir); lerr == nil {
			logMsg = "\n" + logData
		}
		task.Fail(fmt.Errorf("initialize: %w%s", err, logMsg))
		return
	}

	// 5. Write my.ini so mysqld.exe knows where the data lives.
	myini := filepath.Join(installDir, "my.ini")
	iniContent := fmt.Sprintf("[mysqld]\ndatadir=%s\nport=%d\n\n[client]\nport=%d\n",
		dataDir, inst.Port, inst.Port)
	if err := os.WriteFile(myini, []byte(iniContent), 0644); err != nil {
		task.Fail(fmt.Errorf("my.ini: %w", err))
		return
	}

	// 6. Persist install paths back to the instance record.
	m.mu.Lock()
	inst.BinaryDir = installDir
	inst.DataDir = dataDir
	_ = m.saveLocked()
	m.mu.Unlock()

	task.Set(TaskStateDone, fmt.Sprintf("MariaDB %s installed — click Start to launch", version))
}

func resolveMariaDBDownload(ctx context.Context) (version, downloadURL string, err error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, mariadbAPIURL, nil)
	if err != nil {
		return "", "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", "", fmt.Errorf("MariaDB API: %w", err)
	}
	defer resp.Body.Close()

	var data mariadbReleasesResp
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return "", "", fmt.Errorf("parse MariaDB API response: %w", err)
	}

	// Pick the highest patch version that has a Windows x64 ZIP.
	// We don't rely on is_latest (MariaDB API returns it as int 1/0, not bool).
	var bestVer, bestURL string
	for ver, rel := range data.Releases {
		for _, f := range rel.Files {
			if !strings.HasSuffix(strings.ToLower(f.FileName), "-winx64.zip") {
				continue
			}
			if f.DownloadURL == "" {
				continue
			}
			if bestVer == "" || ver > bestVer { // lexicographic comparison works for 11.4.X
				bestVer = ver
				bestURL = f.DownloadURL
			}
		}
	}
	if bestVer != "" {
		return bestVer, bestURL, nil
	}
	return "", "", fmt.Errorf("no Windows x64 package found in MariaDB 11.4 — check https://downloads.mariadb.org/rest-api/mariadb/11.4/")
}

// runDBInit initializes the MariaDB data directory.
// MariaDB's preferred tool is mysql_install_db.exe; if missing, fall back to
// mysqld --initialize-insecure (added for MySQL compatibility in MariaDB 10.4+).
func runDBInit(ctx context.Context, binDir, dataDir string) error {
	installDB := filepath.Join(binDir, "mysql_install_db.exe")
	mysqld := filepath.Join(binDir, "mysqld.exe")

	var cmd *exec.Cmd
	if fileExists(installDB) {
		cmd = exec.CommandContext(ctx, installDB,
			"--datadir="+dataDir,
			"--password=", // empty root password for local dev
		)
	} else {
		cmd = exec.CommandContext(ctx, mysqld,
			"--no-defaults",
			"--datadir="+dataDir,
			"--initialize-insecure",
		)
	}
	// Set working dir to bin so the tool can find its sibling DLLs.
	cmd.Dir = binDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w — %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}

// readFirstErrLog returns the first few lines of the MariaDB error log from dataDir.
func readFirstErrLog(dataDir string) (string, error) {
	entries, err := os.ReadDir(dataDir)
	if err != nil {
		return "", err
	}
	for _, e := range entries {
		if !e.IsDir() && strings.HasSuffix(e.Name(), ".err") {
			data, err := os.ReadFile(filepath.Join(dataDir, e.Name()))
			if err != nil {
				return "", err
			}
			lines := strings.Split(strings.TrimSpace(string(data)), "\n")
			if len(lines) > 20 {
				lines = lines[len(lines)-20:] // last 20 lines
			}
			return strings.Join(lines, "\n"), nil
		}
	}
	return "", fmt.Errorf("no .err log found")
}

// extractZipStrip1 extracts a ZIP, stripping the single top-level directory
// that MariaDB distributions always contain (e.g. mariadb-11.4.4-winx64/).
func extractZipStrip1(zipPath, destDir string) error {
	r, err := zip.OpenReader(zipPath)
	if err != nil {
		return err
	}
	defer r.Close()

	prefix := filepath.Clean(destDir) + string(os.PathSeparator)

	for _, f := range r.File {
		parts := strings.SplitN(filepath.ToSlash(f.Name), "/", 2)
		if len(parts) < 2 || parts[1] == "" {
			continue // skip the root directory entry itself
		}
		outPath := filepath.Join(destDir, filepath.FromSlash(parts[1]))
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
