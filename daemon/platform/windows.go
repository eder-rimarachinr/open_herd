//go:build windows

package platform

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"unicode"
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
	// Try direct write first (works when running as Administrator).
	if f, err := os.OpenFile(hostsFile, os.O_APPEND|os.O_WRONLY, 0644); err == nil {
		_, writeErr := fmt.Fprintf(f, "\r\n%s %s %s\r\n", ip, domain, hostsMarker)
		f.Close()
		if writeErr == nil {
			return nil
		}
	}
	// Fall back: write a temp script and run it elevated (triggers UAC).
	return w.appendHostsEntryElevated(fmt.Sprintf("%s %s %s", ip, domain, hostsMarker))
}

func (w *WindowsPlatform) appendHostsEntryElevated(entry string) error {
	tmp, err := os.CreateTemp("", "phpenv-hosts-*.ps1")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	script := fmt.Sprintf("Add-Content -Path '%s' -Value \"`r`n%s\"", hostsFile, entry)
	tmp.WriteString(script)
	tmp.Close()

	return exec.Command("powershell", "-NonInteractive", "-Command",
		fmt.Sprintf(`Start-Process powershell -Verb RunAs -Wait -ArgumentList '-ExecutionPolicy Bypass -File "%s"'`, tmp.Name()),
	).Run()
}

// ElevatedRun spawns program with its args via PowerShell Start-Process -Verb RunAs,
// which triggers a UAC consent dialog. Blocks until the elevated process exits.
func (w *WindowsPlatform) ElevatedRun(program string, args ...string) error {
	quoted := make([]string, len(args))
	for i, a := range args {
		quoted[i] = fmt.Sprintf("'%s'", psEscape(a))
	}
	var argList string
	if len(quoted) > 0 {
		argList = fmt.Sprintf(" -ArgumentList %s", strings.Join(quoted, ","))
	}
	psCmd := fmt.Sprintf(`Start-Process '%s'%s -Verb RunAs -Wait`, psEscape(program), argList)
	return exec.Command("powershell", "-NonInteractive", "-Command", psCmd).Run()
}

// psEscape escapes single quotes for use inside PowerShell single-quoted strings.
func psEscape(s string) string {
	return strings.Map(func(r rune) rune {
		if r == '\'' {
			return unicode.ReplacementChar // replace with safe char; rare in paths
		}
		return r
	}, s)
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
