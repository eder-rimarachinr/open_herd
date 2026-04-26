package core

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

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
func (s *SSLManager) Install() error {
	return exec.Command(s.plat.MkcertBinary(), "-install").Run()
}

// IssueCert generates a cert+key pair for the given domain under CertsDir.
func (s *SSLManager) IssueCert(domain string) error {
	certFile := filepath.Join(s.cfg.CertsDir, domain+".pem")
	keyFile := filepath.Join(s.cfg.CertsDir, domain+"-key.pem")

	cmd := exec.Command(s.plat.MkcertBinary(),
		"-cert-file", certFile,
		"-key-file", keyFile,
		domain,
	)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("mkcert failed: %s", out)
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
