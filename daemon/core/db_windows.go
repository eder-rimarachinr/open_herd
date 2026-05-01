//go:build windows

package core

import (
	"fmt"
	"log/slog"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
)

// Start starts a managed database instance — via Windows service or direct process.
func (m *DBManager) Start(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot start", inst.Name)
	}
	if inst.ServiceName != "" {
		return m.startService(inst)
	}
	if inst.BinaryDir != "" {
		return m.startProcess(inst)
	}
	return fmt.Errorf("no service name or binary directory configured for %q", inst.Name)
}

// Stop stops a managed database instance.
func (m *DBManager) Stop(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot stop", inst.Name)
	}
	if inst.ServiceName != "" {
		return m.stopService(inst)
	}
	if inst.BinaryDir != "" {
		return m.stopProcess(inst)
	}
	return fmt.Errorf("no service name or binary directory configured for %q", inst.Name)
}

func (m *DBManager) startService(inst *DBInstance) error {
	out, err := exec.Command("sc", "start", inst.ServiceName).CombinedOutput()
	if err != nil {
		if strings.Contains(string(out), "1056") { // already running
			return nil
		}
		return fmt.Errorf("sc start %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

func (m *DBManager) stopService(inst *DBInstance) error {
	out, err := exec.Command("sc", "stop", inst.ServiceName).CombinedOutput()
	if err != nil {
		if strings.Contains(string(out), "1062") { // not started
			return nil
		}
		return fmt.Errorf("sc stop %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

func (m *DBManager) startProcess(inst *DBInstance) error {
	mysqld := filepath.Join(inst.BinaryDir, "bin", "mysqld.exe")
	myini := filepath.Join(inst.BinaryDir, "my.ini")
	cmd := exec.Command(mysqld, "--defaults-file="+myini)
	// Detach from the parent process so it keeps running if the daemon restarts.
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start mysqld: %w", err)
	}
	slog.Info("db: started mysqld process", "pid", cmd.Process.Pid, "instance", inst.Name)
	return nil
}

func (m *DBManager) stopProcess(inst *DBInstance) error {
	mysqladmin := filepath.Join(inst.BinaryDir, "bin", "mysqladmin.exe")
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

// knownServices lists Windows service names to probe when detecting databases.
var knownServices = []struct {
	name   string
	dbType DBType
	port   int
}{
	{"MySQL", DBTypeMySQL, 3306},
	{"MySQL80", DBTypeMySQL, 3306},
	{"MySQL57", DBTypeMySQL, 3306},
	{"MySQL56", DBTypeMySQL, 3306},
	{"MariaDB", DBTypeMariaDB, 3306},
	{"postgresql-x64-17", DBTypePostgres, 5432},
	{"postgresql-x64-16", DBTypePostgres, 5432},
	{"postgresql-x64-15", DBTypePostgres, 5432},
	{"postgresql-x64-14", DBTypePostgres, 5432},
	{"postgresql-x64-13", DBTypePostgres, 5432},
}

// Detect probes known Windows service names and returns instances not yet registered.
func (m *DBManager) Detect() ([]*DBInstance, error) {
	var found []*DBInstance
	for _, svc := range knownServices {
		out, err := exec.Command("sc", "query", svc.name).CombinedOutput()
		if err != nil {
			continue
		}
		if !strings.Contains(string(out), "SERVICE_NAME") {
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
