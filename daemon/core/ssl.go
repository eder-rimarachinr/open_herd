package core

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/open-herd/phpenv/daemon/platform"
)

type SSLManager struct {
	cfg  *Config
	plat platform.Platform
}

func NewSSLManager(cfg *Config, plat platform.Platform) *SSLManager {
	return &SSLManager{cfg: cfg, plat: plat}
}

// Install runs `mkcert -install` to add the local CA to the system trust store.
// If the process is already elevated (admin/root) it runs mkcert directly.
// Otherwise it uses pkexec/sudo on Linux or UAC on Windows.
func (s *SSLManager) Install() error {
	bin := s.plat.MkcertBinary()
	if bin == "" || bin == "mkcert" || bin == "mkcert.exe" {
		if _, err := exec.LookPath(bin); err != nil {
			return fmt.Errorf("mkcert not found — it may not have been downloaded yet")
		}
	}

	if s.plat.IsElevated() {
		// Already admin/root — run directly without a second elevation.
		out, err := exec.Command(bin, "-install").CombinedOutput()
		if err != nil {
			return fmt.Errorf("mkcert -install failed: %s", strings.TrimSpace(string(out)))
		}
		return nil
	}

	// Not elevated — trigger pkexec/UAC.
	return s.plat.ElevatedRun(bin, "-install")
}

// IssueCert generates a cert+key pair for the given domain under CertsDir.
// Returns the actual mkcert output on failure so the caller can show it.
func (s *SSLManager) IssueCert(domain string) error {
	bin := s.plat.MkcertBinary()

	// Verify the binary exists before trying to run it.
	if !fileExists(bin) {
		if _, err := exec.LookPath(bin); err != nil {
			return fmt.Errorf(
				"mkcert binary not found at %q — restart the daemon so it can download it", bin)
		}
	}

	// Ensure the CA root is initialised (creates CAROOT dir if absent).
	// This is a fast no-op if mkcert -install was already run.
	if err := s.ensureCARoot(); err != nil {
		return fmt.Errorf("mkcert CA setup failed: %w", err)
	}

	certFile := filepath.Join(s.cfg.CertsDir, domain+".pem")
	keyFile := filepath.Join(s.cfg.CertsDir, domain+"-key.pem")

	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx,
		bin,
		"-cert-file", certFile,
		"-key-file", keyFile,
		domain,
	)

	out, err := cmd.CombinedOutput()

	if ctx.Err() == context.DeadlineExceeded {
		return fmt.Errorf("mkcert timed out after 60 s — output:\n%s", strings.TrimSpace(string(out)))
	}
	if err != nil {
		return fmt.Errorf("mkcert failed:\n%s", strings.TrimSpace(string(out)))
	}
	return nil
}

// ensureCARoot runs `mkcert -CAROOT` to let mkcert initialise its CA directory.
// This is idempotent and requires no privileges.
func (s *SSLManager) ensureCARoot() error {
	bin := s.plat.MkcertBinary()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	out, err := exec.CommandContext(ctx, bin, "-CAROOT").CombinedOutput()
	if err != nil {
		return fmt.Errorf("%s", strings.TrimSpace(string(out)))
	}
	// Create the CAROOT directory in case mkcert printed the path but didn't create it.
	caroot := strings.TrimSpace(string(out))
	if caroot != "" {
		_ = os.MkdirAll(caroot, 0700)
	}
	return nil
}

// RevokeCert removes the cert files for the given domain.
func (s *SSLManager) RevokeCert(domain string) error {
	certFile := filepath.Join(s.cfg.CertsDir, domain+".pem")
	keyFile := filepath.Join(s.cfg.CertsDir, domain+"-key.pem")

	for _, f := range []string{certFile, keyFile} {
		if err := os.Remove(f); err != nil && !os.IsNotExist(err) {
			return err
		}
	}
	return nil
}

// HasCert reports whether a valid cert exists for the domain.
func (s *SSLManager) HasCert(domain string) bool {
	return fileExists(filepath.Join(s.cfg.CertsDir, domain+".pem"))
}
