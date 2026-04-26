package core

import "github.com/open-herd/phpenv/daemon/platform"

// DNSManager delegates all DNS operations to the platform implementation
// (dnsmasq on Linux, hosts file on Windows).
type DNSManager struct {
	cfg  *Config
	plat platform.Platform
}

func NewDNSManager(cfg *Config, plat platform.Platform) *DNSManager {
	return &DNSManager{cfg: cfg, plat: plat}
}

func (d *DNSManager) AddSite(domain string) error {
	return d.plat.AddHostEntry(domain, "127.0.0.1")
}

func (d *DNSManager) RemoveSite(domain string) error {
	return d.plat.RemoveHostEntry(domain)
}

func (d *DNSManager) Flush() error {
	return d.plat.FlushDNS()
}
