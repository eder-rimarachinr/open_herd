package core

import (
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"
	"github.com/google/uuid"
)

// DirWatcher watches ScannedDirs for subdirectory creation and removal,
// incrementally adding or removing sites without a full scan.
type DirWatcher struct {
	app     *App
	watcher *fsnotify.Watcher
	mu      sync.Mutex
	timers  map[string]*time.Timer
	stop    chan struct{}
}

func NewDirWatcher(app *App) *DirWatcher {
	return &DirWatcher{
		app:    app,
		timers: make(map[string]*time.Timer),
	}
}

func (dw *DirWatcher) Start(dirs []string) error {
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return err
	}
	dw.watcher = w
	dw.stop = make(chan struct{})

	for _, dir := range dirs {
		if err := w.Add(dir); err != nil {
			slog.Warn("dirwatcher: cannot watch dir", "dir", dir, "err", err)
		}
	}

	go dw.run()
	return nil
}

func (dw *DirWatcher) Stop() {
	if dw.watcher == nil {
		return
	}
	close(dw.stop)
	dw.watcher.Close()
	dw.watcher = nil
}

// Restart swaps the watched directory set. Safe to call even if never started.
func (dw *DirWatcher) Restart(dirs []string) {
	dw.Stop()
	if err := dw.Start(dirs); err != nil {
		slog.Error("dirwatcher: restart failed", "err", err)
	}
}

func (dw *DirWatcher) run() {
	for {
		select {
		case <-dw.stop:
			return
		case event, ok := <-dw.watcher.Events:
			if !ok {
				return
			}
			// Only Create/Remove/Rename matter; Write and Chmod are noise.
			if event.Has(fsnotify.Write) || event.Has(fsnotify.Chmod) {
				continue
			}
			dw.debounce(event.Name)
		case err, ok := <-dw.watcher.Errors:
			if !ok {
				return
			}
			slog.Warn("dirwatcher: watcher error", "err", err)
		}
	}
}

// debounce coalesces rapid events on the same path into one processPath call.
func (dw *DirWatcher) debounce(path string) {
	// Only react to immediate subdirectories of scanned dirs.
	parent := filepath.Dir(path)
	watched := false
	for _, dir := range dw.app.Config.ScannedDirs {
		if parent == dir {
			watched = true
			break
		}
	}
	if !watched {
		return
	}

	dw.mu.Lock()
	if t, ok := dw.timers[path]; ok {
		t.Reset(watchDebounce)
	} else {
		dw.timers[path] = time.AfterFunc(watchDebounce, func() {
			dw.mu.Lock()
			delete(dw.timers, path)
			dw.mu.Unlock()
			dw.processPath(path)
		})
	}
	dw.mu.Unlock()
}

func (dw *DirWatcher) processPath(path string) {
	info, err := os.Stat(path)
	if os.IsNotExist(err) {
		dw.removeSite(path)
		return
	}
	if err != nil || !info.IsDir() {
		return
	}
	dw.addSite(path)
}

func (dw *DirWatcher) addSite(path string) {
	for _, s := range dw.app.Sites.List() {
		if s.Path == path {
			return
		}
	}

	name := filepath.Base(path)
	site := &Site{
		ID:          uuid.New().String(),
		Name:        name,
		Domain:      strings.ToLower(name) + DefaultTLD,
		Path:        path,
		PHPVersion:  dw.app.Config.DefaultPHP,
		ProjectType: detectProjectType(path),
		Active:      true,
	}
	if err := dw.app.Sites.Add(site); err != nil {
		slog.Error("dirwatcher: add site failed", "path", path, "err", err)
		return
	}
	_ = dw.app.DNS.AddSite(site.Domain)
	if phpVer, ok := dw.app.PHP.GetVersion(site.PHPVersion); ok {
		_ = dw.app.Nginx.GenerateSiteConfig(site, phpVer.FastCGIAddr)
		if dw.app.Nginx.IsRunning() {
			_ = dw.app.Nginx.Reload()
		}
	}
	slog.Info("dirwatcher: site added", "domain", site.Domain, "path", path)
}

func (dw *DirWatcher) removeSite(path string) {
	var target *Site
	for _, s := range dw.app.Sites.List() {
		if s.Path == path {
			target = s
			break
		}
	}
	if target == nil {
		return
	}
	_ = dw.app.DNS.RemoveSite(target.Domain)
	_ = dw.app.Nginx.RemoveSiteConfig(target.Domain)
	if err := dw.app.Sites.Delete(target.ID); err != nil {
		slog.Error("dirwatcher: delete site failed", "path", path, "err", err)
		return
	}
	if dw.app.Nginx.IsRunning() {
		_ = dw.app.Nginx.Reload()
	}
	slog.Info("dirwatcher: site removed", "domain", target.Domain, "path", path)
}
