package core

import "time"

// Networking — all three components (daemon, CLI, GUI) must agree on DefaultAPIAddr.
const (
	DefaultAPIAddr = "127.0.0.1:7878"
	DefaultTLD     = ".test"
)

// FastCGI port assignment for php-cgi on Windows.
// Formula: FastCGIPortBase + major*10 + minor  →  8.3 = 9083, 7.4 = 9074.
// Minor version is assumed ≤ 9; PHP X.10+ would require a new scheme.
const FastCGIPortBase = 9000

// Process lifecycle delays — tuned empirically, not arbitrary.
const (
	// nginxStartDelay lets nginx bind its socket and write the pid file after cmd.Start().
	nginxStartDelay = 400 * time.Millisecond
	// phpDetectDelay is a brief pause after archive extraction before re-running Detect.
	phpDetectDelay = 500 * time.Millisecond
)

// Timeouts for long-running operations.
const (
	phpInstallTimeout   = 10 * time.Minute
	toolDownloadTimeout = 120 * time.Second
	urlCheckTimeout     = 10 * time.Second
)
