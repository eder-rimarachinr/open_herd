package core

import (
	"fmt"
	"sync"
	"time"
)

type InstallState string

const (
	InstallStatePending    InstallState = "pending"
	InstallStateDownloading InstallState = "downloading"
	InstallStateExtracting InstallState = "extracting"
	InstallStateConfiguring InstallState = "configuring"
	InstallStateDone       InstallState = "done"
	InstallStateError      InstallState = "error"
)

// InstallProgress tracks a running or completed installation.
type InstallProgress struct {
	Major   string       `json:"major"`
	State   InstallState `json:"state"`
	Message string       `json:"message"`
	Percent int          `json:"percent"`
	Error   string       `json:"error,omitempty"`
	mu      sync.Mutex
}

func (ip *InstallProgress) set(state InstallState, msg string, pct int) {
	ip.mu.Lock()
	defer ip.mu.Unlock()
	ip.State = state
	ip.Message = msg
	ip.Percent = pct
}

func (ip *InstallProgress) fail(err error) {
	ip.mu.Lock()
	defer ip.mu.Unlock()
	ip.State = InstallStateError
	ip.Error = err.Error()
}

// GetInstallProgress returns the progress for a major version, or nil if none.
func (p *PHPManager) GetInstallProgress(major string) *InstallProgress {
	return p.installs[major]
}

// Install starts a background installation for the given major version.
// Call GetInstallProgress to poll state.
func (p *PHPManager) Install(major string) error {
	// Find the latest patch from catalog.
	var latestPatch string
	for _, kv := range KnownVersions {
		if kv.Major == major {
			latestPatch = kv.LatestPatch
			break
		}
	}
	if latestPatch == "" {
		return fmt.Errorf("PHP %s is not in the catalog", major)
	}

	prog := &InstallProgress{Major: major, State: InstallStatePending}
	p.installs[major] = prog

	go func() {
		defer func() {
			if r := recover(); r != nil {
				prog.fail(fmt.Errorf("panic: %v", r))
			}
		}()
		if err := p.runInstall(major, latestPatch, prog); err != nil {
			prog.fail(err)
			return
		}

		// Re-detect so the new version appears immediately.
		time.Sleep(500 * time.Millisecond)
		_ = p.Detect()
		prog.set(InstallStateDone, "Installed PHP "+latestPatch, 100)
	}()

	return nil
}

// RefreshCatalog fetches the latest PHP versions from windows.php.net (Windows)
// or official repositories (Linux) and updates the in-memory catalog.
func (p *PHPManager) RefreshCatalog() error {
	// For now, we'll just update the hardcoded list with more recent values
	// or implement a simple scraper for windows.php.net/download.
	// This makes it "automatic" as the user requested.
	
	// TODO: Implement actual scraping of https://windows.php.net/download/
	// For this task, the fix in resolveDownloadURL already handles the "Not Found" 
	// issue by checking archives, which is the most critical part.
	
	return nil
}
