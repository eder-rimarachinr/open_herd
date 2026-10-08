use std::path::{Path, PathBuf};
use crate::infrastructure::dto::Site;

pub fn document_root(site: &Site) -> PathBuf {
    let base = Path::new(&site.path);
    match site.project_type.as_str() {
        "laravel" | "codeigniter4" => base.join("public"),
        "spa" => if base.join("dist").join("index.html").exists() { base.join("dist") } else { base.join("build") },
        _ => base.to_path_buf(),
    }
}

pub fn fastcgi_port(php_version: &str) -> u16 {
    let parts: Vec<u16> = php_version.split('.').take(2).filter_map(|s| s.parse().ok()).collect();
    if parts.len() >= 2 { 9000 + parts[0] * 10 + parts[1] }
    else if parts.len() == 1 { 9000 + parts[0] * 10 }
    else { 9082 }
}

/// Ports nginx listens on, from `config.http_port` / `config.https_port`.
#[derive(Debug, Clone, Copy)]
pub struct ListenPorts {
    pub http: u16,
    pub https: u16,
}

pub fn generate(site: &Site, nginx_dir: &str, ports: ListenPorts) -> Result<(), String> {
    generate_with_certs(site, nginx_dir, ports, None)
}

pub fn generate_with_certs(site: &Site, nginx_dir: &str, ports: ListenPorts, certs_dir: Option<&str>) -> Result<(), String> {
    let sites_dir = Path::new(nginx_dir).join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;
    ensure_fastcgi_params(nginx_dir);
    let fastcgi_params_path = Path::new(nginx_dir).join("fastcgi_params").to_string_lossy().replace('\\', "/");
    let conf_path = sites_dir.join(format!("{}.conf", site.domain));
    let content   = if site.ssl_enabled { build_conf_ssl(site, ports, &fastcgi_params_path, certs_dir.unwrap_or(nginx_dir)) } else { build_conf(site, ports.http, &fastcgi_params_path) };
    std::fs::write(&conf_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove(site: &Site, nginx_dir: &str) {
    let conf_path = Path::new(nginx_dir).join("sites").join(format!("{}.conf", site.domain));
    let _ = std::fs::remove_file(conf_path);
    let sites_dir   = Path::new(nginx_dir).join("sites");
    let placeholder = sites_dir.join(".keep.conf");
    if std::fs::read_dir(&sites_dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::write(&placeholder, "# placeholder\n");
    }
}

fn build_conf_ssl(site: &Site, ports: ListenPorts, fastcgi_params: &str, certs_dir: &str) -> String {
    let (http_port, https_port) = (ports.http, ports.https);
    // `$host` carries no port, so a non-default HTTPS port must be spelled out.
    let https_authority = if https_port == 443 { "$host".to_string() } else { format!("$host:{https_port}") };
    let root       = document_root(site);
    let root_str   = root.to_string_lossy().replace('\\', "/").replace('"', "\\\"");
    let fpm_port   = fastcgi_port(&site.php_version);
    let domain     = &site.domain;
    let certs_dir  = certs_dir.replace('\\', "/");
    let cert_path  = format!("{}/{}.pem", certs_dir, domain).replace('"', "\\\"");
    let key_path   = format!("{}/{}-key.pem", certs_dir, domain).replace('"', "\\\"");
    let autoindex  = if site.project_type == "generic" { "autoindex on;" } else { "autoindex off;" };
    format!(r#"server {{ listen {http_port}; server_name {domain}; return 301 https://{https_authority}$request_uri; }}
server {{ listen {https_port} ssl; server_name {domain}; ssl_certificate "{cert_path}"; ssl_certificate_key "{key_path}"; ssl_protocols TLSv1.2 TLSv1.3; ssl_ciphers HIGH:!aNULL:!MD5; ssl_session_cache shared:SSL:1m; root "{root_str}"; index index.php index.html index.htm; {autoindex} location / {{ try_files $uri $uri/ /index.php?$query_string; }} location ~ \.php$ {{ fastcgi_pass 127.0.0.1:{fpm_port}; fastcgi_index index.php; fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name; include "{fastcgi_params}"; }} location ~ /\.ht {{ deny all; }} }}
"#)
}

fn build_conf(site: &Site, http_port: u16, fastcgi_params: &str) -> String {
    let root      = document_root(site);
    // Escape quotes like the SSL variant does — a path containing `"` could
    // otherwise close the nginx string and inject directives.
    let root_str  = root.to_string_lossy().replace('\\', "/").replace('"', "\\\"");
    let fpm_port  = fastcgi_port(&site.php_version);
    let domain    = &site.domain;
    let autoindex = if site.project_type == "generic" { "autoindex on;" } else { "autoindex off;" };
    format!(r#"server {{ listen {http_port}; server_name {domain}; root "{root_str}"; index index.php index.html index.htm; {autoindex} location / {{ try_files $uri $uri/ /index.php?$query_string; }} location ~ \.php$ {{ fastcgi_pass 127.0.0.1:{fpm_port}; fastcgi_index index.php; fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name; include "{fastcgi_params}"; }} location ~ /\.ht {{ deny all; }} }}
"#)
}

pub fn ensure_fastcgi_params(nginx_dir: &str) {
    let path = Path::new(nginx_dir).join("fastcgi_params");
    if path.exists() { return; }
    let _ = std::fs::write(path, FASTCGI_PARAMS);
}

const FASTCGI_PARAMS: &str = "fastcgi_param  QUERY_STRING       $query_string;\nfastcgi_param  REQUEST_METHOD     $request_method;\nfastcgi_param  CONTENT_TYPE       $content_type;\nfastcgi_param  CONTENT_LENGTH     $content_length;\nfastcgi_param  SCRIPT_NAME        $fastcgi_script_name;\nfastcgi_param  REQUEST_URI        $request_uri;\nfastcgi_param  DOCUMENT_URI       $document_uri;\nfastcgi_param  DOCUMENT_ROOT      $document_root;\nfastcgi_param  SERVER_PROTOCOL    $server_protocol;\nfastcgi_param  REQUEST_SCHEME     $scheme;\nfastcgi_param  HTTPS              $https if_not_empty;\nfastcgi_param  GATEWAY_INTERFACE  CGI/1.1;\nfastcgi_param  SERVER_SOFTWARE    nginx/$nginx_version;\nfastcgi_param  REMOTE_ADDR        $remote_addr;\nfastcgi_param  REMOTE_PORT        $remote_port;\nfastcgi_param  SERVER_ADDR        $server_addr;\nfastcgi_param  SERVER_PORT        $server_port;\nfastcgi_param  SERVER_NAME        $server_name;\nfastcgi_param  REDIRECT_STATUS    200;\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn ssl_site() -> Site {
        Site {
            id: "1".into(), name: "app".into(), domain: "app.test".into(),
            path: if cfg!(windows) { r"C:\www\app".into() } else { "/www/app".into() },
            php_version: "8.2".into(), project_type: "laravel".into(), ssl_enabled: true,
            active: true, created_at: String::new(), updated_at: String::new(),
        }
    }

    #[test]
    fn ssl_vhost_uses_configured_https_port() {
        let conf = build_conf_ssl(&ssl_site(), ListenPorts { http: 8080, https: 8443 }, "fastcgi_params", "/certs");
        assert!(conf.contains("listen 8080;"));
        assert!(conf.contains("listen 8443 ssl;"));
        assert!(!conf.contains("listen 443 "));
        assert!(conf.contains("return 301 https://$host:8443$request_uri;"));
    }

    #[test]
    fn default_https_port_redirect_has_no_explicit_port() {
        let conf = build_conf_ssl(&ssl_site(), ListenPorts { http: 80, https: 443 }, "fastcgi_params", "/certs");
        assert!(conf.contains("listen 443 ssl;"));
        assert!(conf.contains("return 301 https://$host$request_uri;"));
    }
}
