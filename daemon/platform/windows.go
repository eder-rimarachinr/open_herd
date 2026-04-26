//go:build windows

package platform

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

const hostsMarker = "# phpenv"

var hostsFile = filepath.Join(os.Getenv("SystemRoot"), "System32", "drivers", "etc", "hosts")

type WindowsPlatform struct{}

func New() Platform { return &WindowsPlatform{} }

func (w *WindowsPlatform) Name() string { return "windows" }

func (w *WindowsPlatform) AddHostEntry(domain, ip string) error {
	if hostEntryExists(domain) {
		return nil
	}
	f, err := os.OpenFile(hostsFile, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return fmt.Errorf("open %s: %w (try running as Administrator)", hostsFile, err)
	}
	defer f.Close()
	_, err = fmt.Fprintf(f, "%s %s %s\r\n", ip, domain, hostsMarker)
	return err
}

func (w *WindowsPlatform) RemoveHostEntry(domain string) error {
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

	return os.WriteFile(hostsFile, []byte(strings.Join(lines, "\r\n")+"\r\n"), 0644)
}

func (w *WindowsPlatform) FlushDNS() error {
	return exec.Command("ipconfig", "/flushdns").Run()
}

func (w *WindowsPlatform) NginxBinary() string {
	if path, err := exec.LookPath("nginx.exe"); err == nil {
		return path
	}
	home, _ := os.UserHomeDir()
	candidate := filepath.Join(home, ".phpenv", "nginx", "nginx.exe")
	if fileExists(candidate) {
		return candidate
	}
	return "nginx"
}

func (w *WindowsPlatform) MkcertBinary() string {
	if path, err := exec.LookPath("mkcert.exe"); err == nil {
		return path
	}
	home, _ := os.UserHomeDir()
	candidate := filepath.Join(home, ".phpenv", "bin", "mkcert.exe")
	if fileExists(candidate) {
		return candidate
	}
	return "mkcert"
}

func (w *WindowsPlatform) OpenBrowser(url string) error {
	return exec.Command("rundll32", "url.dll,FileProtocolHandler", url).Start()
}

func hostEntryExists(domain string) bool {
	data, err := os.ReadFile(hostsFile)
	if err != nil {
		return false
	}
	return strings.Contains(string(data), domain)
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// isProcessRunning is a stub on Windows — use tasklist or OpenProcess instead.
func isProcessRunning(proc *os.Process) bool {
	// Signal(nil) is not supported on Windows; a full implementation would call
	// OpenProcess(SYNCHRONIZE, pid) and check the return value.
	return proc != nil
}
