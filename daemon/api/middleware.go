package api

import (
	"net/http"
	"net/url"
)

func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		origin := r.Header.Get("Origin")
		// Empty origin means a non-browser client (CLI, curl) — no CORS headers needed.
		if origin != "" && isAllowedOrigin(origin) {
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
			w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
		}

		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// isAllowedOrigin validates the origin using URL parsing so that a hostname
// like "evil-localhost.attacker.com" does not pass the check.
func isAllowedOrigin(origin string) bool {
	u, err := url.Parse(origin)
	if err != nil {
		return false
	}
	host := u.Hostname() // strips port number
	// Allow Tauri native webview (tauri://localhost) and the Vite dev server.
	return u.Scheme == "tauri" || host == "localhost" || host == "127.0.0.1"
}
