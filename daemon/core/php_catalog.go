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

// knownVersions is the master catalogue. Update LatestPatch on new releases.
// A future phase can fetch this list from a remote API instead.
var knownVersions = []struct {
	Major        string
	LatestPatch  string
	SecurityOnly bool
	EndOfLife    bool
}{
	{Major: "8.4", LatestPatch: "8.4.7"},
	{Major: "8.3", LatestPatch: "8.3.21"},
	{Major: "8.2", LatestPatch: "8.2.28"},
	{Major: "8.1", LatestPatch: "8.1.32", SecurityOnly: true},
	{Major: "8.0", LatestPatch: "8.0.30", EndOfLife: true},
	{Major: "7.4", LatestPatch: "7.4.33", EndOfLife: true},
}

// GetCatalog merges the known-version list with what is actually installed.
func (p *PHPManager) GetCatalog() []CatalogEntry {
	catalog := make([]CatalogEntry, 0, len(knownVersions))

	for _, kv := range knownVersions {
		entry := CatalogEntry{
			Major:        kv.Major,
			LatestPatch:  kv.LatestPatch,
			SecurityOnly: kv.SecurityOnly,
			EndOfLife:    kv.EndOfLife,
		}

		if installed, ok := p.versions[kv.Major]; ok {
			entry.Installed = true
			entry.InstalledPatch = installed.Version
			entry.Running = installed.Running
			entry.HasUpdate = installed.Version != kv.LatestPatch
		}

		catalog = append(catalog, entry)
	}

	return catalog
}
