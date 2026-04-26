package core

// CatalogEntry is one PHP version as shown in the GUI — whether installed or not.
type CatalogEntry struct {
	Major          string `json:"major"`
	LatestPatch    string `json:"latest_patch"`
	InstalledPatch string `json:"installed_patch,omitempty"`
	Installed      bool   `json:"installed"`
	Running        bool   `json:"running"`
	HasUpdate      bool   `json:"has_update"`
	SecurityOnly   bool   `json:"security_only"` // active support ended, security patches only
	EndOfLife      bool   `json:"end_of_life"`   // no patches at all
}

// KnownVersions is the master catalogue.
// We use these as a base but try to fetch latest patches dynamically.
var KnownVersions = []struct {
	Major        string
	LatestPatch  string
	SecurityOnly bool
	EndOfLife    bool
}{
	{Major: "8.4", LatestPatch: "8.4.2"},
	{Major: "8.3", LatestPatch: "8.3.14"},
	{Major: "8.2", LatestPatch: "8.2.26"},
	{Major: "8.1", LatestPatch: "8.1.31", SecurityOnly: true},
	{Major: "8.0", LatestPatch: "8.0.30", EndOfLife: true},
	{Major: "7.4", LatestPatch: "7.4.33", EndOfLife: true},
}

// GetCatalog merges the known-version list with what is actually installed.
func (p *PHPManager) GetCatalog() []CatalogEntry {
	catalog := make([]CatalogEntry, 0, len(KnownVersions))

	for _, kv := range KnownVersions {
		entry := CatalogEntry{
			Major:        kv.Major,
			LatestPatch:  kv.LatestPatch,
			SecurityOnly: kv.SecurityOnly,
			EndOfLife:    kv.EndOfLife,
		}

		// Try to find a newer patch if we fetched them dynamically.
		// (Implementation of dynamic fetch can be added to PHPManager struct)

		if installed, ok := p.versions[kv.Major]; ok {
			entry.Installed = true
			entry.InstalledPatch = installed.Version
			entry.Running = installed.Running
			entry.HasUpdate = isNewerVersion(installed.Version, kv.LatestPatch)
		}

		catalog = append(catalog, entry)
	}

	return catalog
}

func isNewerVersion(current, latest string) bool {
	if current == "" || latest == "" {
		return false
	}
	return current != latest
}
