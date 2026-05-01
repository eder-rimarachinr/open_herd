//go:build linux

package core

import (
	"fmt"
	"log/slog"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
)

func (m *DBManager) Start(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot start", inst.Name)
	}
	if inst.ServiceName != "" {
		out, err := exec.Command("systemctl", "start", inst.ServiceName).CombinedOutput()
		if err != nil {
			return fmt.Errorf("systemctl start %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
		}
		return nil
	}
	if inst.BinaryDir != "" {
		return m.startProcess(inst)
	}
	return fmt.Errorf("no service name or binary directory configured for %q", inst.Name)
}

func (m *DBManager) Stop(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot stop", inst.Name)
	}
	if inst.ServiceName != "" {
		out, err := exec.Command("systemctl", "stop", inst.ServiceName).CombinedOutput()
		if err != nil {
			return fmt.Errorf("systemctl stop %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
		}
		return nil
	}
	if inst.BinaryDir != "" {
		return m.stopProcess(inst)
	}
	return fmt.Errorf("no service name or binary directory configured for %q", inst.Name)
}

func (m *DBManager) startProcess(inst *DBInstance) error {
	mysqld := filepath.Join(inst.BinaryDir, "bin", "mysqld")
	myini := filepath.Join(inst.BinaryDir, "my.cnf")
	cmd := exec.Command(mysqld, "--defaults-file="+myini)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start mysqld: %w", err)
	}
	slog.Info("db: started mysqld process", "pid", cmd.Process.Pid, "instance", inst.Name)
	return nil
}

func (m *DBManager) stopProcess(inst *DBInstance) error {
	mysqladmin := filepath.Join(inst.BinaryDir, "bin", "mysqladmin")
	out, err := exec.Command(mysqladmin,
		"-u", "root",
		fmt.Sprintf("--port=%d", inst.Port),
		"--connect-timeout=10",
		"shutdown",
	).CombinedOutput()
	if err != nil {
		return fmt.Errorf("mysqladmin shutdown: %w — %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}

var knownServices = []struct {
	name   string
	dbType DBType
	port   int
}{
	{"mysql", DBTypeMySQL, 3306},
	{"mariadb", DBTypeMariaDB, 3306},
	{"postgresql", DBTypePostgres, 5432},
}

func (m *DBManager) Detect() ([]*DBInstance, error) {
	var found []*DBInstance
	for _, svc := range knownServices {
		out, err := exec.Command("systemctl", "is-enabled", svc.name).CombinedOutput()
		if err != nil || strings.TrimSpace(string(out)) == "not-found" {
			continue
		}
		if m.alreadyRegistered(svc.name) {
			continue
		}
		inst := &DBInstance{
			Name:        svc.name,
			Type:        svc.dbType,
			Host:        "127.0.0.1",
			Port:        svc.port,
			User:        "root",
			Managed:     true,
			ServiceName: svc.name,
		}
		found = append(found, inst)
		slog.Info("db detect: found service", "service", svc.name, "type", svc.dbType)
	}
	return found, nil
}

func (m *DBManager) alreadyRegistered(serviceName string) bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	for _, inst := range m.instances {
		if inst.ServiceName == serviceName {
			return true
		}
	}
	return false
}

// InstallLocal is not yet supported on Linux — databases must be installed via apt/dnf.
func (m *DBManager) InstallLocal(id string) {
	task := m.GetInstallProgress(id)
	if task == nil {
		return
	}
	task.Fail(fmt.Errorf("local install not supported on Linux — use: sudo apt install mariadb-server"))
}
