//go:build linux

package core

import (
	"fmt"
	"log/slog"
	"os/exec"
	"strings"
)

func (m *DBManager) Start(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot start", inst.Name)
	}
	if inst.ServiceName == "" {
		return fmt.Errorf("no service name configured for %q", inst.Name)
	}
	out, err := exec.Command("systemctl", "start", inst.ServiceName).CombinedOutput()
	if err != nil {
		return fmt.Errorf("systemctl start %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

func (m *DBManager) Stop(inst *DBInstance) error {
	if !inst.Managed {
		return fmt.Errorf("%q is a remote connection — cannot stop", inst.Name)
	}
	if inst.ServiceName == "" {
		return fmt.Errorf("no service name configured for %q", inst.Name)
	}
	out, err := exec.Command("systemctl", "stop", inst.ServiceName).CombinedOutput()
	if err != nil {
		return fmt.Errorf("systemctl stop %s: %w — %s", inst.ServiceName, err, strings.TrimSpace(string(out)))
	}
	return nil
}

var knownServices = []struct {
	name   string
	dbType DBType
	port   int
}{
	{"mysql",      DBTypeMySQL,    3306},
	{"mariadb",    DBTypeMariaDB,  3306},
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
