package core

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
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
// On Windows this triggers a certificate-trust dialog; on Linux it uses pkexec/sudo.
func (s *SSLManager) Install() error {
	return s.plat.ElevatedRun(s.plat.MkcertBinary(), "-install")
}



// IssueCert generates a cert+key pair for the given domain under CertsDir.
func (s *SSLManager) IssueCert(domain string) error {
	certFile := filepath.Join(s.cfg.CertsDir, domain+".pem")
	keyFile := filepath.Join(s.cfg.CertsDir, domain+"-key.pem")

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, s.plat.MkcertBinary(),
		"-cert-file", certFile,
		"-key-file", keyFile,
		domain,
	)
	
	logPath := filepath.Join(s.cfg.CertsDir, domain+"-mkcert.log")
	logFile, _ := os.Create(logPath)
	if logFile != nil {
		cmd.Stdout = logFile
		cmd.Stderr = logFile
		defer logFile.Close()
	}

	err := cmd.Run()
	
	if ctx.Err() == context.DeadlineExceeded {
		return fmt.Errorf("mkcert timed out (el generador se quedo bloqueado 15s)")
	}
	if err != nil {
		return fmt.Errorf("mkcert failed, check log: %s", logPath)
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
