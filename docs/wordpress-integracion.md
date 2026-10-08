# Integración de WordPress en Open Herd

Objetivo: poder servir sitios **WordPress** (en particular la copia local de la tienda WooCommerce **emove.pe**, que hoy
corre en LocalWP) desde Open Herd, con dominio `.test`, HTTPS con mkcert y la versión de PHP elegida por sitio.

Este documento tiene tres partes:

1. [Qué ya funciona y qué falta](#1-qué-ya-funciona-y-qué-falta) (estado del código a la versión 0.1.1).
2. [Procedimiento manual para hoy](#2-procedimiento-manual-para-hoy-sin-cambios-en-open-herd): mover la copia de LocalWP a Open Herd sin tocar el código.
3. [Plan de implementación](#3-plan-de-implementación-en-open-herd): qué agregar a Open Herd, siguiendo su arquitectura hexagonal.

---

## 1. Qué ya funciona y qué falta

### Ya funciona

| Pieza | Dónde | Notas |
|---|---|---|
| Detección del tipo `wordpress` | `application/site/project_type_detector.rs` | Por `wp-config.php` o `wp-login.php` |
| Información del sitio | `infrastructure/site_info/mod.rs` → `read_wordpress` | Lee `DB_NAME`, `WP_HOME`/`WP_SITEURL` y la versión de `wp-includes/version.php` |
| Vhost de nginx | `infrastructure/nginx/vhost_config.rs` | `try_files $uri $uri/ /index.php?$query_string` → los enlaces permanentes de WordPress funcionan |
| HTTPS | mkcert + `fastcgi_param HTTPS $https` | WordPress detecta `is_ssl()` correctamente |
| Dominio `.test` | `HostsAdapter` | |
| PHP por sitio + edición de `php.ini` | `application/php/ini_parser.rs`, `PUT /api/v1/php/versions/:version/ini` | Ya expone `upload_max_filesize`, `post_max_size`, `memory_limit`, `max_execution_time`, `max_input_time`, `max_input_vars` y la activación de extensiones |

### Falta (brechas)

| # | Brecha | Efecto en WordPress | Prioridad |
|---|---|---|---|
| G1 | **No hay base de datos** (MySQL/MariaDB) | WordPress no arranca ("Error al establecer una conexión con la base de datos") | Bloqueante |
| G2 | **nginx no define `client_max_body_size`** (en `infrastructure/nginx/process.rs`, bloque `http`) | El valor por defecto de nginx es **1 MB**: fallan con **413** las subidas de medios, plugins, temas y copias `.wpress`, aunque `php.ini` permita más | Alta |
| G3 | Sin `fastcgi_read_timeout` / `fastcgi_send_timeout` | Importaciones largas (All-in-One, WooCommerce) se cortan a los 60 s por defecto | Media |
| G4 | Extensiones de PHP que WordPress/WooCommerce necesitan pueden venir desactivadas | `mysqli` es obligatoria; sin `curl`, `gd`, `mbstring`, `intl`, `zip`, `openssl`, `fileinfo`, `exif`, `xml` hay avisos o fallos en *Salud del sitio* | Media (se resuelve desde la UI de PHP si cada extensión existe en la build) |
| G5 | Sin WP-CLI | Sin `wp search-replace` (cambio de dominio), `wp user update`, etc. | Media |
| G6 | Sin plantilla "Crear sitio WordPress" | Hay que descargar WordPress y escribir `wp-config.php` a mano | Baja |
| G7 | Sin captura de correo | Una copia de producción puede enviar correos reales | Baja (se cubre con `wp-config.php`, ver §2.6) |

---

## 2. Procedimiento manual para hoy (sin cambios en Open Herd)

Caso: mover la copia **emotesys** de LocalWP a Open Herd como `https://emotesys.test`.

| Dato de origen (LocalWP) | Valor |
|---|---|
| Archivos | `D:\proyectos\WORDPRESS\emotesys\app\public` |
| Base de datos | MySQL 8.4 de Local, puerto `10095`, BD `local`, `root`/`root`, prefijo `wp_` |
| PHP | 8.3.29 |
| URL actual | `http://localhost:10094` (fijada en `wp-config.php` con `WP_HOME`/`WP_SITEURL`) |

### 2.1 Base de datos (G1)

Mientras Open Herd no gestione una base de datos, usar una instalada aparte:

- **MariaDB** (recomendado; es el motor de producción de EMOVE) como servicio de Windows, o la versión portable (zip) en
  `C:\mariadb`, escuchando en `127.0.0.1:3306`.
- Alternativa temporal: seguir usando el MySQL de LocalWP (`127.0.0.1:10095`) con Local abierto. Sirve para probar,
  pero deja Open Herd dependiendo de Local.

Crear la base y el usuario:

```sql
CREATE DATABASE emotesys CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
CREATE USER 'emotesys'@'localhost' IDENTIFIED BY '<clave>';
GRANT ALL PRIVILEGES ON emotesys.* TO 'emotesys'@'localhost';
FLUSH PRIVILEGES;
```

### 2.2 Exportar desde LocalWP e importar

Con el sitio arrancado en Local:

```bat
:: Exportar (usa el mysqldump de Local o cualquier cliente 8.x)
mysqldump -h 127.0.0.1 -P 10095 -u root -proot --single-transaction --default-character-set=utf8mb4 local > emotesys.sql

:: Importar en la base nueva
mysql -h 127.0.0.1 -P 3306 -u emotesys -p emotesys < emotesys.sql
```

> MySQL 8 usa la intercalación `utf8mb4_0900_ai_ci`, que MariaDB no tiene. Si la importación falla por eso, reemplazar en
> el `.sql` `utf8mb4_0900_ai_ci` por `utf8mb4_unicode_ci` antes de importar.

> La copia contiene **datos personales de clientes** (pedidos, emails, teléfonos). No dejar el `.sql` dentro de un repo
> y borrarlo al terminar.

### 2.3 Archivos

Copiar `D:\proyectos\WORDPRESS\emotesys\app\public` a la carpeta donde Open Herd buscará el sitio, por ejemplo
`D:\sites\emotesys`. (No mover el original hasta comprobar que la copia funciona.)

### 2.4 `wp-config.php`

En la copia:

```php
define( 'DB_NAME', 'emotesys' );
define( 'DB_USER', 'emotesys' );
define( 'DB_PASSWORD', '<clave>' );
define( 'DB_HOST', '127.0.0.1:3306' );

// Reemplaza las líneas de localhost:10094 que puso LocalWP
define( 'WP_HOME', 'https://emotesys.test' );
define( 'WP_SITEURL', 'https://emotesys.test' );
```

Mantener el bloque de **copia segura** que ya tiene (WordPress no sale a internet ni ejecuta su cron):

```php
define( 'WP_HTTP_BLOCK_EXTERNAL', true );
define( 'WP_ACCESSIBLE_HOSTS', '*.local,*.test' ); // añadir *.test
define( 'DISABLE_WP_CRON', true );
```

### 2.5 Open Herd

1. **Sitios → agregar** `D:\sites\emotesys` con dominio `emotesys.test`. Debe detectarse como `wordpress`.
2. **PHP**: asignar **8.3** e instalarla si no está.
3. **PHP → php.ini** de 8.3: `upload_max_filesize = 8192M`, `post_max_size = 8192M`, `memory_limit = 1024M` (8192M solo
   para importar copias grandes), `max_execution_time = 1200`, `max_input_time = 600`, `max_input_vars = 5000`
   (WooCommerce guarda formularios grandes). Activar las extensiones de G4.
4. **nginx (G2/G3)**: mientras no exista la opción, editar a mano el `nginx.conf` generado
   (`~/.phpenv/nginx/nginx.conf`, o `./data/nginx/nginx.conf` en modo portable) y añadir dentro de `http { ... }`:
   ```nginx
   client_max_body_size 8192M;
   fastcgi_read_timeout 1200s;
   fastcgi_send_timeout 1200s;
   ```
   y recargar nginx. **Ojo:** Open Herd regenera ese archivo; el cambio puede perderse al reiniciar nginx desde la app
   hasta implementar la Fase 2 del §3.
5. **SSL**: activar HTTPS para `emotesys.test` (mkcert).

### 2.6 Cambio de dominio dentro de la base (G5)

Las URL guardadas en la base (contenido, opciones serializadas) siguen apuntando al dominio anterior. Con
[WP-CLI](https://wp-cli.org) (`wp-cli.phar` + PHP 8.3), desde la carpeta del sitio:

```bat
php wp-cli.phar search-replace "http://localhost:10094" "https://emotesys.test" --all-tables --precise
php wp-cli.phar search-replace "http://emotesys.local" "https://emotesys.test" --all-tables --precise
php wp-cli.phar search-replace "https://emove.pe" "https://emotesys.test" --all-tables --precise --dry-run   :: revisar antes de aplicar
php wp-cli.phar cache flush
php wp-cli.phar rewrite flush
```

Luego, en el panel: *Ajustes → Enlaces permanentes → Guardar*. Si WP Rocket avisa de que cambió el dominio, pulsar
"Regenerar archivos de configuración".

Acceso al panel: `https://emotesys.test/wp-admin`. Para restablecer la contraseña:
`php wp-cli.phar user update localadmin --user_pass="<nueva>"`.

### 2.7 Conectar el backend de EMOVE

En `emobe_be/.env`:

```env
WC_API=https://emotesys.test/wp-json/wc/v3
```

`WC_CID` / `WC_CCLI` no cambian (las claves REST viven en la base, que se copió).

> **Certificado:** el cliente de WooCommerce de EMOVE (`src/automated-tasks/wc-api.client.ts`) solo acepta certificados
> no oficiales en hosts `*.local`. Para `*.test` con mkcert hay dos opciones:
> - Ejecutar el backend con `NODE_EXTRA_CA_CERTS=<ruta a rootCA.pem de mkcert>` (`mkcert -CAROOT` muestra la carpeta).
>   Es la opción recomendada: no toca código.
> - O ampliar en ese cliente la excepción a `*.test` (cambio de código en `emobe_be`).

Comprobar con `npx ts-node --files scripts/wc-assign-skus.ts` (solo lectura) o con el botón "SINCRONIZAR CON WC"
(simulación).

### 2.8 Verificación

- [ ] `https://emotesys.test` carga la tienda y `https://emotesys.test/wp-admin` permite entrar.
- [ ] *Herramientas → Salud del sitio* sin errores críticos (extensiones, `mysqli`).
- [ ] *All-in-One WP Migration → Importar* muestra el nuevo límite de subida (no 300 MB / 1 MB).
- [ ] `https://emotesys.test/wp-json/wc/v3/products` responde con las claves REST.
- [ ] El backend de EMOVE lista los productos (35 en la copia actual).
- [ ] Ninguna URL vieja: `php wp-cli.phar search-replace "localhost:10094" "x" --dry-run` reporta 0 reemplazos.

---

## 3. Plan de implementación en Open Herd

Cada fase sigue la arquitectura del proyecto: **puerto** en `domain/ports/`, **adaptador** en `infrastructure/`,
**caso de uso** por operación en `application/`, cableado en `AppContainer`, **handler** delgado en
`ports/http/*_handlers.rs` y ruta en `ports/http/server.rs`. Sin `.unwrap()`/`.expect()` en `domain/` y
`application/` (lints de clippy). Tests: `cargo test --lib` (dominio + casos de uso) y `cargo test` (integración con
`axum-test`).

### Fase 1 — Límites de subida de nginx (G2, G3) · pequeña, mayor impacto

- `config.json`: nuevos campos `nginx_client_max_body_size` (por defecto `"1024M"`) y `nginx_fastcgi_timeout_s`
  (por defecto `600`).
- `infrastructure/nginx/process.rs`: escribirlos en el bloque `http { ... }` del `nginx.conf` generado.
- `GET/PUT /api/v1/config` ya existe: exponer los campos y mostrarlos en *Settings*.
- Validación igual que `validate_setting` de PHP (`128M`, `1G`, `8192M`).
- Test: el `nginx.conf` generado contiene `client_max_body_size` con el valor configurado.

### Fase 2 — Base de datos gestionada (G1) · la más grande

- **Puerto** `DatabasePort` (`domain/ports/`): `install(version)`, `start()`, `stop()`, `status()`,
  `create_database(name, user, password)`, `list_databases()`.
- **Adaptador** `MariaDbAdapter` (`infrastructure/database/`):
  - Windows: descargar el zip oficial de MariaDB (mismo patrón que la descarga de nginx, con `DownloadProgressPort`),
    datos en `~/.phpenv/mariadb/data`, `mariadb-install-db.exe` la primera vez, proceso `mariadbd.exe` en
    `127.0.0.1:3306` (puerto configurable).
  - Linux: usar el paquete del sistema (como nginx) o el binario portable.
- **Casos de uso**: `InstallDatabaseUseCase`, `StartDatabaseUseCase`, `StopDatabaseUseCase`,
  `CreateDatabaseUseCase` (valida nombres: solo `[A-Za-z0-9_]`, como `validate_extension_name`).
- **Rutas**: `GET /api/v1/database/status`, `POST /api/v1/database/install`, `GET /api/v1/database/install/progress`,
  `POST /api/v1/database/start|stop`, `GET/POST /api/v1/database/databases`.
- **Servicios**: incluir la base en `POST /api/v1/services/start|stop` junto a nginx y PHP.
- **GUI**: página *Base de datos* (estado, iniciar/detener, crear BD y usuario).
- Seguridad: la base escucha solo en `127.0.0.1`; `local_guard` ya protege la API.

### Fase 3 — WP-CLI (G5)

- Descargar `wp-cli.phar` a `~/.phpenv/bin/` y generar `wp.bat` / `wp` que lo ejecute con el PHP del sitio.
- Endpoint opcional `POST /api/v1/sites/:id/wp` con una lista blanca de comandos (`search-replace`, `cache flush`,
  `rewrite flush`, `user update`) para usarlos desde la GUI; **nunca** pasar argumentos sin validar a una shell.

### Fase 4 — Sitio WordPress nuevo y ajustes por tipo (G4, G6)

- Caso de uso `CreateWordPressSiteUseCase`: descarga la última versión de WordPress, crea la base (Fase 2), escribe
  `wp-config.php` con claves aleatorias (`AUTH_KEY`, …) y registra el sitio.
- Al detectar un sitio `wordpress`: avisar en la GUI si faltan extensiones de G4 o si `upload_max_filesize` es menor que
  `client_max_body_size`.

### Fase 5 — Importar copias (opcional)

- Asistente "Importar sitio WordPress": carpeta de archivos + `.sql`, crea la base, importa, ajusta `wp-config.php` y
  ejecuta `search-replace` al dominio `.test`.

---

## Referencias

- Guía de la copia local actual (LocalWP) y sus ajustes: `emobe_be/docs/entorno-local-woocommerce.md`.
- Integración EMOVE ↔ WooCommerce: `emobe_be/src/automated-tasks/README.md`.
