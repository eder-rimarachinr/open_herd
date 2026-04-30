package core

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
)

// PHPVersion describes an installed PHP version.
// On Linux: FPM via Unix socket. On Windows: php-cgi via TCP port.
type PHPVersion struct {
	Version     string `json:"version"`
	Major       string `json:"major"`
	BinaryPath  string `json:"binary_path"`
	FPMBinary   string `json:"fpm_binary"`   // php-fpm (Linux) or php-cgi.exe (Windows)
	FastCGIAddr string `json:"fastcgi_addr"` // "unix:/path" on Linux, "127.0.0.1:PORT" on Windows
	FPMPidFile  string `json:"fpm_pid_file"`
	Installed   bool   `json:"installed"`
	Running     bool   `json:"running"`
}

type PHPManager struct {
	mu       sync.RWMutex
	cfg      *Config
	versions map[string]*PHPVersion
	// installs tracks in-progress or completed installs.
	installs map[string]*InstallProgress
	// procs tracks running php-cgi processes on Windows (no PID file).
	procs map[string]*os.Process
}

func NewPHPManager(cfg *Config) *PHPManager {
	return &PHPManager{
		cfg:      cfg,
		versions: make(map[string]*PHPVersion),
		installs: make(map[string]*InstallProgress),
		procs:    make(map[string]*os.Process),
	}
}

// Detect scans well-known paths, common installers (XAMPP, WAMP, Laragon),
// the system PATH, and user-configured custom dirs for PHP binaries.
// I/O runs without holding the lock; only the final map swap is protected.
func (p *PHPManager) Detect() error {
	found := make(map[string]*PHPVersion)

	for _, dir := range p.searchPaths() {
		log.Printf("PHP Detect: Scanning dir %s", dir)
		entries, err := os.ReadDir(dir)
		if err != nil {
			log.Printf("PHP Detect: Error reading dir %s: %v", dir, err)
			continue
		}
		for _, entry := range entries {
			if entry.IsDir() {
				p.detectInDir(filepath.Join(dir, entry.Name()), found)
				continue
			}
			if isPHPBinary(entry.Name()) {
				log.Printf("PHP Detect: Found binary %s in %s", entry.Name(), dir)
				p.detectBinary(filepath.Join(dir, entry.Name()), found)
			}
		}
	}

	for _, name := range []string{"php", "php.exe"} {
		if resolved, err := exec.LookPath(name); err == nil {
			p.detectBinary(resolved, found)
		}
	}

	p.mu.Lock()
	p.versions = found
	p.mu.Unlock()
	return nil
}

func (p *PHPManager) detectInDir(dir string, into map[string]*PHPVersion) {
	p.detectBinary(filepath.Join(dir, phpBinaryName()), into)
}

func (p *PHPManager) detectBinary(binaryPath string, into map[string]*PHPVersion) {
	if !fileExists(binaryPath) {
		return
	}
	out, err := exec.Command(binaryPath, "-r", "echo PHP_VERSION;").Output()
	if err != nil {
		log.Printf("PHP Detect: Execution failed %s: %v", binaryPath, err)
		return
	}
	full := strings.TrimSpace(string(out))
	log.Printf("PHP Detect: Detected version %s at %s", full, binaryPath)

	parts := strings.SplitN(full, ".", 3)
	if len(parts) < 2 {
		return
	}
	major := parts[0] + "." + parts[1]

	if _, exists := into[major]; exists {
		return // first one wins
	}

	dir := filepath.Dir(binaryPath)
	into[major] = &PHPVersion{
		Version:     full,
		Major:       major,
		BinaryPath:  binaryPath,
		FPMBinary:   p.findFPMBinary(dir, major),
		FastCGIAddr: p.fastCGIAddr(major),
		FPMPidFile:  filepath.Join(p.cfg.BaseDir, fmt.Sprintf("php%s-fpm.pid", major)),
		Installed:   true,
	}
}

// fastCGIAddr returns the address nginx should use for this PHP version.
// Linux → Unix socket path; Windows → TCP loopback with fixed port.
func (p *PHPManager) fastCGIAddr(major string) string {
	if runtime.GOOS == "windows" {
		port := majorToPort(major)
		return fmt.Sprintf("127.0.0.1:%d", port)
	}
	return "unix:" + filepath.Join(p.cfg.BaseDir, fmt.Sprintf("php%s.sock", major))
}

// majorToPort derives a fixed FastCGI port from a PHP major version string.
// E.g. "8.3" → 9083, "7.4" → 9074.
func majorToPort(major string) int {
	parts := strings.SplitN(major, ".", 2)
	if len(parts) != 2 {
		return FastCGIPortBase
	}
	var minor int
	fmt.Sscanf(parts[1], "%d", &minor)
	var maj int
	fmt.Sscanf(parts[0], "%d", &maj)
	return FastCGIPortBase + maj*10 + minor
}

func (p *PHPManager) searchPaths() []string {
	var paths []string
	
	// 1. Managed PHP directory (always checked first)
	paths = append(paths, p.cfg.PHPDir)

	// 2. All other paths (XAMPP, manual installs, etc) come from config
	// so the user can see and manage them in the GUI.
	paths = append(paths, p.cfg.CustomPHPDirs...)
	
	return paths
}

func (p *PHPManager) findFPMBinary(dir, major string) string {
	var candidates []string
	if runtime.GOOS == "windows" {
		// Windows uses php-cgi.exe instead of php-fpm.
		candidates = []string{
			filepath.Join(dir, "php-cgi.exe"),
		}
	} else {
		candidates = []string{
			filepath.Join(dir, "php-fpm"+major),
			filepath.Join(dir, "php-fpm"),
			filepath.Join(filepath.Dir(dir), "sbin", "php-fpm"+major),
			filepath.Join(filepath.Dir(dir), "sbin", "php-fpm"),
		}
	}
	for _, c := range candidates {
		if fileExists(c) {
			return c
		}
	}
	return ""
}

func (p *PHPManager) GetVersions() []*PHPVersion {
	p.mu.RLock()
	defer p.mu.RUnlock()
	result := make([]*PHPVersion, 0, len(p.versions))
	for _, v := range p.versions {
		result = append(result, v)
	}
	return result
}

func (p *PHPManager) GetVersion(major string) (*PHPVersion, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	if major == "" {
		major = p.cfg.DefaultPHP
	}
	if major == "" {
		for k := range p.versions {
			major = k
			break
		}
	}
	v, ok := p.versions[major]
	return v, ok
}

// StartFPM starts php-fpm (Linux) or php-cgi (Windows) for the given major version.
func (p *PHPManager) StartFPM(major string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	v, ok := p.versions[major]
	if !ok {
		return fmt.Errorf("PHP %s not installed", major)
	}
	if v.FPMBinary == "" {
		return fmt.Errorf("no FastCGI binary found for PHP %s", major)
	}
	if v.Running {
		return nil
	}

	if runtime.GOOS == "windows" {
		port := majorToPort(major)
		// -b and the address must be separate arguments; a combined string won't split.
		cmd := exec.Command(v.FPMBinary, "-b", fmt.Sprintf("127.0.0.1:%d", port))
		if err := cmd.Start(); err != nil {
			return err
		}
		p.procs[major] = cmd.Process
		v.Running = true
		return nil
	}

	cfgPath := filepath.Join(p.cfg.PHPDir, major, "php-fpm.conf")
	if err := p.generateFPMConfig(v, cfgPath); err != nil {
		return err
	}
	cmd := exec.Command(v.FPMBinary, "--fpm-config", cfgPath, "--daemonize")
	if err := cmd.Start(); err != nil {
		return err
	}
	v.Running = true
	return nil
}

// StopFPM stops the FastCGI process for the given major version.
func (p *PHPManager) StopFPM(major string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	v, ok := p.versions[major]
	if !ok {
		return fmt.Errorf("PHP %s not found", major)
	}

	// On Windows, php-cgi doesn't write a PID file — use the tracked process.
	if runtime.GOOS == "windows" {
		proc, running := p.procs[major]
		if !running {
			v.Running = false
			return ErrNotRunning
		}
		_ = proc.Kill()
		delete(p.procs, major)
		v.Running = false
		return nil
	}

	data, err := os.ReadFile(v.FPMPidFile)
	if err != nil {
		v.Running = false
		return ErrNotRunning
	}

	var pid int
	fmt.Sscanf(strings.TrimSpace(string(data)), "%d", &pid)
	proc, err := os.FindProcess(pid)
	if err != nil {
		return err
	}
	if err := proc.Kill(); err != nil {
		return err
	}
	v.Running = false
	_ = os.Remove(v.FPMPidFile)
	return nil
}

func (p *PHPManager) generateFPMConfig(v *PHPVersion, configPath string) error {
	if err := os.MkdirAll(filepath.Dir(configPath), 0755); err != nil {
		return err
	}
	socketPath := strings.TrimPrefix(v.FastCGIAddr, "unix:")
	content := fmt.Sprintf(`[global]
pid = %s
error_log = %s

[www]
listen = %s
listen.mode = 0660
pm = dynamic
pm.max_children = 5
pm.start_servers = 2
pm.min_spare_servers = 1
pm.max_spare_servers = 3
`,
		v.FPMPidFile,
		filepath.Join(p.cfg.LogsDir, fmt.Sprintf("php-fpm-%s.log", v.Major)),
		socketPath,
	)
	return os.WriteFile(configPath, []byte(content), 0644)
}

func isPHPBinary(name string) bool {
	lower := strings.ToLower(name)
	return lower == "php" || lower == "php.exe"
}

func phpBinaryName() string {
	if runtime.GOOS == "windows" {
		return "php.exe"
	}
	return "php"
}
