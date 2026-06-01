use std::path::{Path, PathBuf};
use super::models::Site;

/// Returns the document root for a site based on its project type
pub fn document_root(site: &Site) -> PathBuf {
    let base = Path::new(&site.path);
    match site.project_type.as_str() {
        "laravel" | "codeigniter4" => base.join("public"),
        "spa" => {
            if base.join("dist").join("index.html").exists() {
                base.join("dist")
            } else {
                base.join("build")
            }
        }
        _ => base.to_path_buf(),
    }
}

/// PHP FastCGI port for a version string like "8.2" or "8.2.1"
pub fn fastcgi_port(php_version: &str) -> u16 {
    let parts: Vec<u16> = php_version
        .split('.')
        .take(2)
        .filter_map(|s| s.parse().ok())
        .collect();
    if parts.len() >= 2 {
        9000 + parts[0] * 10 + parts[1]
    } else if parts.len() == 1 {
        9000 + parts[0] * 10
    } else {
        9082 // fallback: PHP 8.2
    }
}

pub fn generate(site: &Site, nginx_dir: &str, http_port: u16) -> Result<(), String> {
    generate_with_certs(site, nginx_dir, http_port, None)
}

pub fn generate_with_certs(
    site: &Site,
    nginx_dir: &str,
    http_port: u16,
    certs_dir: Option<&str>,
) -> Result<(), String> {
    let sites_dir = Path::new(nginx_dir).join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;

    ensure_fastcgi_params(nginx_dir);
    let fastcgi_params_path = Path::new(nginx_dir)
        .join("fastcgi_params")
        .to_string_lossy()
        .replace('\\', "/");

    let conf_path = sites_dir.join(format!("{}.conf", site.domain));
    let content = if site.ssl_enabled {
        let certs = certs_dir.unwrap_or(nginx_dir);
        build_conf_ssl(site, http_port, &fastcgi_params_path, certs)
    } else {
        build_conf(site, http_port, &fastcgi_params_path)
    };
    std::fs::write(&conf_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove(site: &Site, nginx_dir: &str) {
    let conf_path = Path::new(nginx_dir)
        .join("sites")
        .join(format!("{}.conf", site.domain));
    let _ = std::fs::remove_file(conf_path);

    // Nginx include "sites/*.conf" fails if the directory is empty.
    // Keep a harmless placeholder so nginx doesn't error on reload.
    let sites_dir = Path::new(nginx_dir).join("sites");
    let placeholder = sites_dir.join(".keep.conf");
    if std::fs::read_dir(&sites_dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::write(&placeholder, "# placeholder\n");
    }
}

fn build_conf_ssl(site: &Site, http_port: u16, fastcgi_params: &str, certs_dir: &str) -> String {
    let root = document_root(site);
    let root_str = root.to_string_lossy().replace('\\', "/");
    let fpm_port = fastcgi_port(&site.php_version);
    let domain = &site.domain;
    let certs_dir = certs_dir.replace('\\', "/");

    // Escape any single/double quotes in the paths (defensive)
    let cert_path = format!("{}/{}.pem",     certs_dir, domain).replace('"', "\\\"");
    let key_path  = format!("{}/{}-key.pem", certs_dir, domain).replace('"', "\\\"");
    let root_str  = root_str.replace('"', "\\\"");

    let autoindex = match site.project_type.as_str() {
        "generic" => "autoindex on;",
        _ => "autoindex off;",
    };

    format!(
        r#"# HTTP → HTTPS redirect
server {{
    listen       {http_port};
    server_name  {domain};
    return 301   https://$host$request_uri;
}}

# HTTPS
server {{
    listen       443 ssl;
    server_name  {domain};

    ssl_certificate     "{cert_path}";
    ssl_certificate_key "{key_path}";
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;
    ssl_session_cache   shared:SSL:1m;

    root  "{root_str}";
    index index.php index.html index.htm;

    {autoindex}

    location / {{
        try_files $uri $uri/ /index.php?$query_string;
    }}

    location ~ \.php$ {{
        fastcgi_pass   127.0.0.1:{fpm_port};
        fastcgi_index  index.php;
        fastcgi_param  SCRIPT_FILENAME  $document_root$fastcgi_script_name;
        include        "{fastcgi_params}";
    }}

    location ~ /\.ht {{
        deny all;
    }}
}}
"#
    )
}

fn build_conf(site: &Site, http_port: u16, fastcgi_params: &str) -> String {
    let root = document_root(site);
    let root_str = root.to_string_lossy().replace('\\', "/");
    let fpm_port = fastcgi_port(&site.php_version);
    let domain = &site.domain;

    let autoindex = match site.project_type.as_str() {
        "generic" => "autoindex on;",
        _ => "autoindex off;",
    };

    format!(
        r#"server {{
    listen       {http_port};
    server_name  {domain};

    root  "{root_str}";
    index index.php index.html index.htm;

    {autoindex}

    location / {{
        try_files $uri $uri/ /index.php?$query_string;
    }}

    location ~ \.php$ {{
        fastcgi_pass   127.0.0.1:{fpm_port};
        fastcgi_index  index.php;
        fastcgi_param  SCRIPT_FILENAME  $document_root$fastcgi_script_name;
        include        "{fastcgi_params}";
    }}

    location ~ /\.ht {{
        deny all;
    }}
}}
"#
    )
}

/// Write the fastcgi_params file into nginx_dir if it doesn't exist
pub fn ensure_fastcgi_params(nginx_dir: &str) {
    let path = Path::new(nginx_dir).join("fastcgi_params");
    if path.exists() { return; }
    let _ = std::fs::write(path, FASTCGI_PARAMS);
}

const FASTCGI_PARAMS: &str = r#"fastcgi_param  QUERY_STRING       $query_string;
fastcgi_param  REQUEST_METHOD     $request_method;
fastcgi_param  CONTENT_TYPE       $content_type;
fastcgi_param  CONTENT_LENGTH     $content_length;
fastcgi_param  SCRIPT_NAME        $fastcgi_script_name;
fastcgi_param  REQUEST_URI        $request_uri;
fastcgi_param  DOCUMENT_URI       $document_uri;
fastcgi_param  DOCUMENT_ROOT      $document_root;
fastcgi_param  SERVER_PROTOCOL    $server_protocol;
fastcgi_param  REQUEST_SCHEME     $scheme;
fastcgi_param  HTTPS              $https if_not_empty;
fastcgi_param  GATEWAY_INTERFACE  CGI/1.1;
fastcgi_param  SERVER_SOFTWARE    nginx/$nginx_version;
fastcgi_param  REMOTE_ADDR        $remote_addr;
fastcgi_param  REMOTE_PORT        $remote_port;
fastcgi_param  SERVER_ADDR        $server_addr;
fastcgi_param  SERVER_PORT        $server_port;
fastcgi_param  SERVER_NAME        $server_name;
fastcgi_param  REDIRECT_STATUS    200;
"#;
