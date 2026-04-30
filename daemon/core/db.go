package core

import (
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/google/uuid"
)

type DBType string

const (
	DBTypeMySQL    DBType = "mysql"
	DBTypeMariaDB  DBType = "mariadb"
	DBTypePostgres DBType = "postgres"
)

// DBInstance is either a locally managed database server or a remote connection profile.
type DBInstance struct {
	ID          string    `json:"id"`
	Name        string    `json:"name"`
	Type        DBType    `json:"type"`
	Host        string    `json:"host"`
	Port        int       `json:"port"`
	User        string    `json:"user"`
	Password    string    `json:"password,omitempty"`
	Managed     bool      `json:"managed"`                  // true = phpenv can start/stop it
	ServiceName string    `json:"service_name,omitempty"`   // Windows service or systemd unit
	CreatedAt   time.Time `json:"created_at"`
}

type DBManager struct {
	cfg       *Config
	mu        sync.RWMutex
	instances map[string]*DBInstance
}

func NewDBManager(cfg *Config) *DBManager {
	return &DBManager{cfg: cfg, instances: make(map[string]*DBInstance)}
}

func (m *DBManager) Load() error {
	path := filepath.Join(m.cfg.BaseDir, "databases.json")
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	var list []*DBInstance
	if err := json.Unmarshal(data, &list); err != nil {
		return err
	}
	for _, inst := range list {
		m.instances[inst.ID] = inst
	}
	return nil
}

func (m *DBManager) saveLocked() error {
	list := make([]*DBInstance, 0, len(m.instances))
	for _, inst := range m.instances {
		list = append(list, inst)
	}
	data, err := json.MarshalIndent(list, "", "  ")
	if err != nil {
		return err
	}
	finalPath := filepath.Join(m.cfg.BaseDir, "databases.json")
	tmpPath := finalPath + ".tmp"
	if err := os.WriteFile(tmpPath, data, 0644); err != nil {
		return err
	}
	return os.Rename(tmpPath, finalPath)
}

func (m *DBManager) List() []*DBInstance {
	m.mu.RLock()
	defer m.mu.RUnlock()
	list := make([]*DBInstance, 0, len(m.instances))
	for _, inst := range m.instances {
		list = append(list, inst)
	}
	return list
}

func (m *DBManager) Get(id string) (*DBInstance, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	inst, ok := m.instances[id]
	return inst, ok
}

func (m *DBManager) Add(inst *DBInstance) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if inst.ID == "" {
		inst.ID = uuid.New().String()
	}
	inst.CreatedAt = time.Now()
	if inst.Host == "" {
		inst.Host = "127.0.0.1"
	}
	if inst.Port == 0 {
		inst.Port = DefaultDBPort(inst.Type)
	}
	if inst.User == "" {
		inst.User = "root"
	}
	m.instances[inst.ID] = inst
	return m.saveLocked()
}

func (m *DBManager) Delete(id string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if _, ok := m.instances[id]; !ok {
		return ErrNotFound
	}
	delete(m.instances, id)
	return m.saveLocked()
}

// IsRunning checks if the database port is accepting TCP connections.
func (m *DBManager) IsRunning(inst *DBInstance) bool {
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("%s:%d", inst.Host, inst.Port), time.Second)
	if err != nil {
		return false
	}
	conn.Close()
	return true
}

func DefaultDBPort(t DBType) int {
	switch t {
	case DBTypeMySQL, DBTypeMariaDB:
		return 3306
	case DBTypePostgres:
		return 5432
	default:
		return 0
	}
}
