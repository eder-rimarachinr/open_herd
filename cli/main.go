package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"time"

	"github.com/spf13/cobra"
)

const daemonAddr = "http://127.0.0.1:7878"

func main() {
	root := &cobra.Command{
		Use:   "phpenv",
		Short: "PHP environment manager CLI",
	}

	root.AddCommand(
		cmdOpen(),
		cmdStop(),
		cmdStatus(),
		cmdSites(),
		cmdPHP(),
		cmdNginx(),
	)

	if err := root.Execute(); err != nil {
		os.Exit(1)
	}
}

// ── open ──────────────────────────────────────────────────────────────────────

func cmdOpen() *cobra.Command {
	var noGUI bool
	cmd := &cobra.Command{
		Use:   "open",
		Short: "Start all services and open the GUI (like double-clicking Herd)",
		RunE: func(cmd *cobra.Command, args []string) error {
			// 1. Ensure daemon is running.
			if !isDaemonRunning() {
				fmt.Println("Starting phpenv daemon...")
				if err := spawnDaemon(); err != nil {
					return fmt.Errorf("could not start daemon: %w\n\nRun manually: phpenv-daemon", err)
				}
				if err := waitForDaemon(10 * time.Second); err != nil {
					return fmt.Errorf("daemon did not respond in time: %w", err)
				}
				fmt.Println("Daemon ready.")
			}

			// 2. Start services (nginx + PHP-FPM + DNS).
			fmt.Println("Starting services...")
			if err := postJSON(daemonAddr + "/api/v1/services/start"); err != nil {
				fmt.Fprintf(os.Stderr, "Warning: %v\n", err)
			}

			// 3. Open the GUI.
			if !noGUI {
				guiURL := "http://localhost:1420"
				fmt.Printf("Opening GUI at %s\n", guiURL)
				_ = openURL(guiURL)
			}
			return nil
		},
	}
	cmd.Flags().BoolVar(&noGUI, "no-gui", false, "Start services without opening the GUI")
	return cmd
}

// ── stop ──────────────────────────────────────────────────────────────────────

func cmdStop() *cobra.Command {
	var quit bool
	cmd := &cobra.Command{
		Use:   "stop",
		Short: "Stop all services (nginx + PHP-FPM)",
		RunE: func(cmd *cobra.Command, args []string) error {
			if !isDaemonRunning() {
				fmt.Println("phpenv daemon is not running.")
				return nil
			}
			if quit {
				fmt.Println("Stopping all services and quitting daemon...")
				_ = postJSON(daemonAddr + "/api/v1/daemon/quit")
				fmt.Println("Done.")
				return nil
			}
			fmt.Println("Stopping services...")
			return postJSON(daemonAddr + "/api/v1/services/stop")
		},
	}
	cmd.Flags().BoolVar(&quit, "quit", false, "Also stop the daemon process")
	return cmd
}

// ── status ────────────────────────────────────────────────────────────────────

func cmdStatus() *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show daemon and services status",
		RunE: func(cmd *cobra.Command, args []string) error {
			if !isDaemonRunning() {
				fmt.Println("phpenv daemon is not running. Use: phpenv open")
				return nil
			}
			return printJSON(daemonAddr + "/api/v1/services/status")
		},
	}
}

// ── sites ─────────────────────────────────────────────────────────────────────

func cmdSites() *cobra.Command {
	cmd := &cobra.Command{Use: "sites", Short: "Manage sites"}
	cmd.AddCommand(
		&cobra.Command{
			Use:   "list",
			Short: "List all registered sites",
			RunE: func(cmd *cobra.Command, args []string) error {
				return printJSON(daemonAddr + "/api/v1/sites")
			},
		},
		&cobra.Command{
			Use:   "scan",
			Short: "Discover new sites in scanned directories",
			RunE: func(cmd *cobra.Command, args []string) error {
				return postJSON(daemonAddr + "/api/v1/sites/scan")
			},
		},
	)
	return cmd
}

// ── php ───────────────────────────────────────────────────────────────────────

func cmdPHP() *cobra.Command {
	cmd := &cobra.Command{Use: "php", Short: "Manage PHP versions"}
	cmd.AddCommand(
		&cobra.Command{
			Use:  "versions",
			RunE: func(cmd *cobra.Command, args []string) error { return printJSON(daemonAddr + "/api/v1/php/versions") },
		},
		&cobra.Command{
			Use:  "start [version]",
			Args: cobra.ExactArgs(1),
			RunE: func(cmd *cobra.Command, args []string) error {
				return postJSON(fmt.Sprintf("%s/api/v1/php/versions/%s/start", daemonAddr, args[0]))
			},
		},
		&cobra.Command{
			Use:  "stop [version]",
			Args: cobra.ExactArgs(1),
			RunE: func(cmd *cobra.Command, args []string) error {
				return postJSON(fmt.Sprintf("%s/api/v1/php/versions/%s/stop", daemonAddr, args[0]))
			},
		},
	)
	return cmd
}

// ── nginx ─────────────────────────────────────────────────────────────────────

func cmdNginx() *cobra.Command {
	cmd := &cobra.Command{Use: "nginx", Short: "Control nginx"}
	for _, action := range []string{"start", "stop", "reload"} {
		a := action
		cmd.AddCommand(&cobra.Command{
			Use:  a,
			RunE: func(cmd *cobra.Command, args []string) error { return postJSON(daemonAddr + "/api/v1/nginx/" + a) },
		})
	}
	cmd.AddCommand(&cobra.Command{
		Use:  "status",
		RunE: func(cmd *cobra.Command, args []string) error { return printJSON(daemonAddr + "/api/v1/nginx/status") },
	})
	return cmd
}

// ── daemon helpers ────────────────────────────────────────────────────────────

func isDaemonRunning() bool {
	client := &http.Client{Timeout: 500 * time.Millisecond}
	resp, err := client.Get(daemonAddr + "/api/v1/status")
	if err != nil {
		return false
	}
	resp.Body.Close()
	return resp.StatusCode == http.StatusOK
}

func waitForDaemon(timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if isDaemonRunning() {
			return nil
		}
		time.Sleep(300 * time.Millisecond)
	}
	return errors.New("timed out waiting for daemon")
}

func spawnDaemon() error {
	bin := findDaemonBinary()
	if bin == "" {
		return errors.New("phpenv-daemon binary not found")
	}
	cmd := exec.Command(bin)
	cmd.Stdout = nil
	cmd.Stderr = nil
	cmd.Stdin = nil
	if err := cmd.Start(); err != nil {
		return err
	}
	// Detach: let the child outlive this process.
	return cmd.Process.Release()
}

func findDaemonBinary() string {
	// 1. Same directory as the CLI binary.
	self, err := os.Executable()
	if err == nil {
		candidate := filepath.Join(filepath.Dir(self), daemonBinaryName())
		if fileExists(candidate) {
			return candidate
		}
	}
	// 2. ~/.phpenv/bin/
	home, _ := os.UserHomeDir()
	installed := filepath.Join(home, ".phpenv", "bin", daemonBinaryName())
	if fileExists(installed) {
		return installed
	}
	// 3. PATH
	if path, err := exec.LookPath(daemonBinaryName()); err == nil {
		return path
	}
	return ""
}

func daemonBinaryName() string {
	if runtime.GOOS == "windows" {
		return "phpenv-daemon.exe"
	}
	return "phpenv-daemon"
}

func openURL(url string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	case "darwin":
		cmd = exec.Command("open", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	return cmd.Start()
}

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

func printJSON(url string) error {
	resp, err := http.Get(url) //nolint:gosec
	if err != nil {
		return fmt.Errorf("daemon unreachable: %w", err)
	}
	defer resp.Body.Close()
	return prettyPrint(resp.Body)
}

func postJSON(url string) error {
	resp, err := http.Post(url, "application/json", nil) //nolint:gosec
	if err != nil {
		return fmt.Errorf("daemon unreachable: %w", err)
	}
	defer resp.Body.Close()
	return prettyPrint(resp.Body)
}

func prettyPrint(r io.Reader) error {
	var v any
	if err := json.NewDecoder(r).Decode(&v); err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}
