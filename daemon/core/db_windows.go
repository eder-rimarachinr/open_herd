//go:build windows

package core

import (
	"fmt"
	"log/slog"
	"os/exec"
	"strings"
)

// Start starts a managed database Windows service.
func (m *DBManager) Start(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot start", inst.Name)
	}
	if inst.ServiceName == "" {
		return fmt.Errorf("no service name configured for %q", inst.Name)
	}
	out, err := exec.Command("sc", "start", inst.ServiceName).CombinedOutput()
	if err != nil {
		// Exit code 1056 means "already running" — treat as success.
		if strings.Contains(string(out), "1056") {
			return nil
		}
		return fmt.Errorf("sc start %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

// Stop stops a managed database Windows service.
func (m *DBManager) Stop(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot stop", inst.Name)
	}
	if inst.ServiceName == "" {
		return fmt.Errorf("no service name configured for %q", inst.Name)
	}
	out, err := exec.Command("sc", "stop", inst.ServiceName).CombinedOutput()
	if err != nil {
		// Exit code 1062 means "not started" — treat as success.
		if strings.Contains(string(out), "1062") {
			return nil
		}
		return fmt.Errorf("sc stop %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

// knownServices lists Windows service names to probe when detecting databases.
var knownServices = []struct {
	name    string
	dbType  DBType
	port    int
}{
	{"MySQL",   DBTypeMySQL,    3306},
	{"MySQL80", DBTypeMySQL,    3306},
	{"MySQL57", DBTypeMySQL,    3306},
	{"MySQL56", DBTypeMySQL,    3306},
	{"MariaDB", DBTypeMariaDB,  3306},
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
			continue // service doesn't exist
		}
		if !strings.Contains(string(out), "SERVICE_NAME") {
			continue
		}
		// Skip if already registered.
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
