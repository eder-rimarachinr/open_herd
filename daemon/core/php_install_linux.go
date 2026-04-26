//go:build linux

package core

import (
	"fmt"
	"os/exec"
	"strings"
)

// runInstall installs PHP via the ondrej/php PPA on Debian/Ubuntu.
// For other distros (RHEL/Fedora) it falls back to dnf/yum.
func (p *PHPManager) runInstall(major, version string, prog *InstallProgress) error {
	prog.set(InstallStateDownloading, "Checking package manager…", 5)

	if commandExists("apt-get") {
		return p.installApt(major, prog)
	}
	if commandExists("dnf") {
		return p.installDnf(major, prog)
	}
	if commandExists("yum") {
		return p.installYum(major, prog)
	}

	return fmt.Errorf("no supported package manager found (apt, dnf, yum)")
}

func (p *PHPManager) installApt(major string, prog *InstallProgress) error {
	noMajorDot := strings.ReplaceAll(major, ".", "") // "8.3" → "83"
	_ = noMajorDot

	// Add ondrej/php PPA if not already present.
	prog.set(InstallStateDownloading, "Adding ondrej/php PPA…", 10)
	if err := run("sudo", "add-apt-repository", "-y", "ppa:ondrej/php"); err != nil {
		// Non-fatal — repo might already exist.
		_ = err
	}

	prog.set(InstallStateDownloading, "Updating package index…", 20)
	if err := run("sudo", "apt-get", "update", "-qq"); err != nil {
		return fmt.Errorf("apt-get update: %w", err)
	}

	packages := []string{
		fmt.Sprintf("php%s", major),
		fmt.Sprintf("php%s-fpm", major),
		fmt.Sprintf("php%s-common", major),
		fmt.Sprintf("php%s-cli", major),
		fmt.Sprintf("php%s-mbstring", major),
		fmt.Sprintf("php%s-xml", major),
		fmt.Sprintf("php%s-curl", major),
		fmt.Sprintf("php%s-zip", major),
		fmt.Sprintf("php%s-mysql", major),
		fmt.Sprintf("php%s-pgsql", major),
		fmt.Sprintf("php%s-sqlite3", major),
		fmt.Sprintf("php%s-gd", major),
		fmt.Sprintf("php%s-intl", major),
	}

	prog.set(InstallStateExtracting, fmt.Sprintf("Installing PHP %s packages…", major), 40)
	args := append([]string{"apt-get", "install", "-y", "-qq"}, packages...)
	if err := run("sudo", args...); err != nil {
		return fmt.Errorf("apt-get install: %w", err)
	}

	prog.set(InstallStateConfiguring, "Done", 95)
	return nil
}

func (p *PHPManager) installDnf(major string, prog *InstallProgress) error {
	pkg := fmt.Sprintf("php%s-php-fpm", strings.ReplaceAll(major, ".", ""))
	prog.set(InstallStateExtracting, fmt.Sprintf("dnf install %s…", pkg), 30)
	return run("sudo", "dnf", "install", "-y", pkg)
}

func (p *PHPManager) installYum(major string, prog *InstallProgress) error {
	pkg := fmt.Sprintf("php%s-php-fpm", strings.ReplaceAll(major, ".", ""))
	prog.set(InstallStateExtracting, fmt.Sprintf("yum install %s…", pkg), 30)
	return run("sudo", "yum", "install", "-y", pkg)
}

func run(name string, args ...string) error {
	out, err := exec.Command(name, args...).CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w: %s", err, out)
	}
	return nil
}

func commandExists(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}
