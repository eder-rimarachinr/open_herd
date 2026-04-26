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
	for _, kv := range knownVersions {
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
