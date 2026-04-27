//go:build linux

package platform

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

const hostsFile = "/etc/hosts"
const hostsMarker = "# phpenv"

type LinuxPlatform struct{}

func New() Platform { return &LinuxPlatform{} }

func (l *LinuxPlatform) Name() string { return "linux" }

func (l *LinuxPlatform) AddHostEntry(domain, ip string) error {
	if hostEntryExists(domain) {
		return nil
	}
	entry := fmt.Sprintf("\n%s %s %s\n", ip, domain, hostsMarker)

	// Try direct write first (works if daemon is running as root).
	if f, err := os.OpenFile(hostsFile, os.O_APPEND|os.O_WRONLY, 0644); err == nil {
		_, writeErr := fmt.Fprintf(f, entry)
		f.Close()
		if writeErr == nil {
			return nil
		}
	}

	// Fall back: pipe the entry into tee with elevated privileges.
	teeArgs := []string{"tee", "-a", hostsFile}
	var cmd *exec.Cmd
	if _, err := exec.LookPath("pkexec"); err == nil {
		cmd = exec.Command("pkexec", teeArgs...)
	} else {
		cmd = exec.Command("sudo", teeArgs...)
	}
	cmd.Stdin = strings.NewReader(entry)
	return cmd.Run()
}

// ElevatedRun runs program with args using pkexec (GUI password dialog on desktop
// environments) falling back to sudo (terminal). Blocks until exit.
func (l *LinuxPlatform) ElevatedRun(program string, args ...string) error {
	all := append([]string{program}, args...)
	if _, err := exec.LookPath("pkexec"); err == nil {
		return exec.Command("pkexec", all...).Run()
	}
	return exec.Command("sudo", all...).Run()
}

func (l *LinuxPlatform) RemoveHostEntry(domain string) error {
	data, err := os.ReadFile(hostsFile)
	if err != nil {
		return err
	}

	var lines []string
	scanner := bufio.NewScanner(strings.NewReader(string(data)))
	for scanner.Scan() {
		line := scanner.Text()
		if strings.Contains(line, domain) && strings.Contains(line, hostsMarker) {
			continue
		}
		lines = append(lines, line)
	}

	return os.WriteFile(hostsFile, []byte(strings.Join(lines, "\n")+"\n"), 0644)
}

func (l *LinuxPlatform) FlushDNS() error {
	// Reload dnsmasq if present; otherwise the hosts file change is immediate.
	if _, err := exec.LookPath("dnsmasq"); err == nil {
		return exec.Command("systemctl", "reload", "dnsmasq").Run()
	}
	return nil
}

func (l *LinuxPlatform) NginxBinary() string {
	if path, err := exec.LookPath("nginx"); err == nil {
		return path
	}
	return "nginx"
}

func (l *LinuxPlatform) MkcertBinary() string {
	if path, err := exec.LookPath("mkcert"); err == nil {
		return path
	}
	return "mkcert"
}

func (l *LinuxPlatform) IsElevated() bool {
	return os.Getuid() == 0
}

func (l *LinuxPlatform) OpenBrowser(url string) error {
	return exec.Command("xdg-open", url).Start()
}

func hostEntryExists(domain string) bool {
	data, err := os.ReadFile(hostsFile)
	if err != nil {
		return false
	}
	return strings.Contains(string(data), domain)
}

// isProcessRunning checks if a process is alive by sending signal 0.
func isProcessRunning(proc *os.Process) bool {
	err := proc.Signal(os.Signal(nil))
	return err == nil
}
