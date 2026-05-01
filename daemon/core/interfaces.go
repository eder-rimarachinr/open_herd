package core

// SiteStore is the persistence layer for registered sites.
type SiteStore interface {
	Load() error
	List() []*Site
	Get(id string) (*Site, bool)
	Add(site *Site) error
	Update(site *Site) error
	Delete(id string) error
	Scan() ([]*Site, error)
	AddMultiple(sites []*Site) error
}

// PHPRuntime manages installed PHP versions and their FastCGI processes.
type PHPRuntime interface {
	Detect() error
	GetVersions() []*PHPVersion
	GetVersion(major string) (*PHPVersion, bool)
	GetCatalog() []CatalogEntry
	StartFPM(major string) error
	StopFPM(major string) error
	Install(major string) error
	GetInstallProgress(major string) *InstallProgress
}

// NginxController manages the nginx process and its configuration files.
type NginxController interface {
	IsInstalled() bool
	IsRunning() bool
	Downloadable() bool
	Download() error
	StartDownloadTask() *AsyncTask
	GetDownloadProgress() *AsyncTask
	Start() error
	Stop() error
	Reload() error
	Test() error
	Version() string
	ErrorLogTail(lines int) string
	GenerateMainConfig() error
	GenerateSiteConfig(site *Site, fastCGIAddr string) error
	RemoveSiteConfig(domain string) error
}

// DNSController manages /etc/hosts entries for .test domains.
type DNSController interface {
	AddSite(domain string) error
	RemoveSite(domain string) error
	Flush() error
}

// CertManager wraps mkcert for local certificate issuance.
type CertManager interface {
	Install() error
	IssueCert(domain string) error
	RevokeCert(domain string) error
	HasCert(domain string) bool
	StartCertTask(domain string) *AsyncTask
	GetCertTask(domain string) *AsyncTask
}

// DBController manages local and remote database instances.
type DBController interface {
	Load() error
	List() []*DBInstance
	Get(id string) (*DBInstance, bool)
	Add(inst *DBInstance) error
	Delete(id string) error
	IsRunning(inst *DBInstance) bool
	Start(inst *DBInstance) error
	Stop(inst *DBInstance) error
	Detect() ([]*DBInstance, error)
	// InstallLocal downloads and initialises a portable DB inside the phpenv data dir.
	// It is async: call StartInstallTask first, then launch InstallLocal in a goroutine.
	InstallLocal(id string)
	StartInstallTask(id string) *AsyncTask
	GetInstallProgress(id string) *AsyncTask
}

// Compile-time checks: concrete types must satisfy their interfaces.
var (
	_ SiteStore       = (*SiteManager)(nil)
	_ PHPRuntime      = (*PHPManager)(nil)
	_ NginxController = (*NginxManager)(nil)
	_ DNSController   = (*DNSManager)(nil)
	_ CertManager     = (*SSLManager)(nil)
	_ DBController    = (*DBManager)(nil)
)
