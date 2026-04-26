package platform

// Platform abstracts OS-specific operations so the core package stays portable.
type Platform interface {
	Name() string

	// DNS / hosts
	AddHostEntry(domain, ip string) error
	RemoveHostEntry(domain string) error
	FlushDNS() error

	// Binaries (resolved at runtime to handle PATH, managed installs, etc.)
	NginxBinary() string
	MkcertBinary() string

	// Shell helpers
	OpenBrowser(url string) error

	// ElevatedRun runs program with elevated privileges (UAC on Windows,
	// pkexec/sudo on Linux). It blocks until the child exits.
	ElevatedRun(program string, args ...string) error
}
