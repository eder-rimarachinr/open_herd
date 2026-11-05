# Página "Node": versiones de Node.js en Open Herd (Windows)

## Intención

El usuario usa nvm para cambiar la versión de Node según el proyecto que está
trabajando. Quiere hacer ese cambio desde Open Herd, igual que elige la versión
de PHP por defecto: **una sola versión activa en todo el sistema**, que cambia a
mano cuando pasa a otro proyecto. Todas las terminales nuevas usan esa versión.

## Objetivos

- Ver las versiones de Node instaladas y cuál está activa.
- Activar otra versión con un clic, sin permisos de administrador.
- Instalar versiones estables desde nodejs.org con una lista **leída en vivo**
  (`https://nodejs.org/dist/index.json`), no fijada en el código (el catálogo de
  PHP fijado hubo que actualizarlo a mano por los CVE de octubre de 2026).
- Comprobar cada zip descargado contra el `SHASUMS256.txt` de su release.
- Desinstalar versiones que no estén activas.

## Fuera de alcance

- Versión de Node por sitio, `.nvmrc` o cambio automático al abrir un sitio
  (el usuario lo descartó: la versión es del sistema).
- Linux y macOS (se dejan comentarios `TODO(linux)` y el adaptador responde
  "no soportado"; nada a medio hacer).
- Gestionar npm/yarn/pnpm, paquetes globales o corepack.
- Comprobar la firma GPG de `SHASUMS256.txt`: se confía en HTTPS de nodejs.org,
  igual que hace nvm-windows.

## Anti-objetivos (sería un fracaso aunque "funcione")

- Romper el nvm que el usuario ya tiene: después de usar Open Herd,
  `nvm list` / `nvm use` deben seguir funcionando y ver las mismas versiones.
- Pedir permisos de administrador o mostrar un UAC.
- Borrar una carpeta real que no sea un enlace o un junction.
- Que los tests toquen el PATH real, `C:\nvm4w` o el registro.

## Dos modos (decidido con el usuario)

> "si ya hay un gestor como nvm, open herd hace cambios sobre ese, pero si no
> existe, open herd manda"

### Modo nvm-windows (hay nvm instalado)

Se detecta con `NVM_HOME` y `NVM_SYMLINK`. Primero se leen las variables del
proceso y, si faltan, el registro (`HKCU\Environment` y luego
`HKLM\...\Session Manager\Environment`), porque el proceso puede tener un
entorno antiguo. Como respaldo se lee `%NVM_HOME%\settings.txt` (claves
`root:` y `path:`).

- **Almacén de versiones:** el mismo de nvm, `NVM_HOME\vX.Y.Z\` (con `node.exe`
  en la raíz de esa carpeta; el zip trae el prefijo `node-vX.Y.Z-win-x64/`, que se
  quita al extraer). Así nvm ve lo que instala Open Herd, y Open Herd ve lo que
  se instaló con nvm.
- **Activar:** se reapunta `NVM_SYMLINK` (en la máquina del usuario,
  `C:\nvm4w\nodejs`) a `NVM_HOME\vX.Y.Z` con un **junction**. Un junction no
  necesita administrador; basta con permiso de escritura en la carpeta padre, y
  "Usuarios autentificados" tiene Modify en `C:\nvm4w`. nvm-windows usa un
  symlink, pero `nvm use` borra con `rmdir` y vuelve a crear el enlace, así que
  sigue funcionando sobre un junction. No se toca el PATH (nvm ya puso
  `NVM_SYMLINK` en él).
- Si `NVM_SYMLINK` existe y **no** es un enlace ni un junction (es una carpeta
  real), se rechaza la activación con un error claro y no se borra nada.
- Si `NVM_SYMLINK` no existe (`nvm off`), se crea, lo que equivale a `nvm on`.
- La versión activa es el destino del junction o symlink, sin ejecutar `node -v`.

Hay otros nvm (por ejemplo el que trae Laravel Herd en
`~/.config/herd/bin/nvm`) que no están detrás de `NVM_HOME`. Se ignoran.

### Modo Open Herd (no hay nvm)

- **Almacén:** `<base_dir>/node/vX.Y.Z/` (`base_dir` = `~/.phpenv` o `./data` en
  modo portable).
- **Activar:** junction `<base_dir>/node/current` → `vX.Y.Z`.
- **PATH:** la primera vez que se activa algo, se añade `<base_dir>\node\current`
  al PATH de usuario (`HKCU\Environment\Path`, conservando su tipo
  `REG_EXPAND_SZ`) si no está ya, y se avisa con `WM_SETTINGCHANGE` para que las
  terminales nuevas lo vean. Nunca se toca el PATH de máquina.
- **Aviso:** si en el PATH de máquina ya hay otro `node.exe` (por ejemplo
  `C:\Program Files\nodejs` del instalador MSI), irá por delante del de usuario.
  En ese caso el estado devuelve un aviso que la página muestra ("Otro Node en
  el PATH del sistema tapa al de Open Herd: …").

## Lista de versiones disponibles

Se descarga `index.json` (con caché en memoria de 1 h y botón "Actualizar") y
se filtra con una **función pura** de la capa de aplicación, con tests:

- Se descartan las prerreleases (versión con `-`, p. ej. `v27.0.0-alpha…`) y las
  que no traen `win-x64-zip` en `files`.
- Por cada línea mayor se queda la release más reciente.
- Se muestran las líneas **LTS** (`lts` ≠ `false`) que siguen mantenidas, más la
  línea "Current" más nueva. Criterio de "mantenida": su última release tiene
  menos de 6 meses. `index.json` no trae fechas de fin de vida, y una línea viva
  saca releases cada pocas semanas.
- Cada entrada lleva: mayor, versión, nombre LTS o `null`, fecha, `security`,
  si está instalada (versión exacta) y si hay una más nueva instalable dentro de
  la misma mayor ya instalada.

Si nodejs.org no responde, `GET /node/releases` falla con un error que la página
muestra, pero la lista de instaladas y el botón de activar siguen funcionando.

## Arquitectura (sigue `CLAUDE.md`)

- **Dominio:** valor `NodeVersion` (validado `^\d+\.\d+\.\d+$`, con o sin `v`;
  evita path traversal en rutas y carpetas). Puertos en `domain/ports/node.rs`:
  - `NodeManagerPort`: `status()` (modo, carpeta del almacén, ruta del enlace,
    instaladas, activa, avisos), `activate(&NodeVersion)`,
    `uninstall(&NodeVersion)` y `install_dir()`. Toca el sistema (junction, PATH,
    registro), así que va en **`SystemAdapters`** y en los tests se usa un doble.
  - `NodeReleasesPort`: `releases()` devuelve las entradas crudas de `index.json`
    (red). También es inyectable en `SystemAdapters` para que los tests no
    salgan a internet.
  - `DownloadProgressPort` gana `start_node_download(version, dest_dir)` y
    `node_progress(version)`. La descarga resuelve
    `https://nodejs.org/dist/vX.Y.Z/node-vX.Y.Z-win-x64.zip`, lee el hash de
    `SHASUMS256.txt` y reutiliza `fetch_zip` (marca `.install-incomplete`, quita
    el prefijo, borra el zip temporal). Una instalación con la marca no cuenta
    como instalada.
- **Aplicación** (`application/node/`): `GetNodeStatus`, `ListNodeReleases`
  (aplica el filtro), `InstallNode` (no repite una descarga en curso; rechaza si
  ya está instalada), `ActivateNode` (exige que esté instalada y completa) y
  `UninstallNode` (rechaza la versión activa y las descargas en curso).
- **Infraestructura** (`infrastructure/node/`): `WindowsNodeManager` (detección
  nvm/propio, junction con el crate `junction`, PATH con `winreg` o
  `windows-sys`, todo dentro de `infrastructure::blocking::run`) y
  `HttpNodeReleases` (reqwest async + caché). En no-Windows,
  `NodeManagerPort` responde "no soportado" (`TODO(linux)`).
- **HTTP** (`ports/http/node_handlers.rs`, handlers finos con `ApiJson` y
  `app_error`):

  ```text
  GET    /api/v1/node                              estado (modo, activa, instaladas, avisos)
  GET    /api/v1/node/releases[?refresh=1]         versiones disponibles
  POST   /api/v1/node/install         {version}    → TaskState
  GET    /api/v1/node/install/{version}/progress
  POST   /api/v1/node/versions/{version}/activate
  DELETE /api/v1/node/versions/{version}
  ```

- **DTOs** con ts-rs (`NodeStatus`, `NodeInstall`, `NodeRelease`, modo como
  enum espejo en `ts_bindings`), reexportados en `src/api/client.ts`. Se añaden
  las rutas a la lista de `CLAUDE.md`.

## Interfaz

- Entrada "Node" en la barra lateral, justo debajo de PHP, con un icono de la
  misma librería.
- `src/pages/Node.tsx` con los estilos de PHP (`PHP.module.css` /
  `Page.module.css`):
  - **Cabecera:** versión activa grande, modo ("Gestionado con nvm-windows en
    `C:\nvm4w\nodejs`" u "Open Herd"), avisos y la nota "Las terminales que abras
    a partir de ahora usarán esta versión".
  - **Instaladas:** tabla con versión, LTS, el botón "Activar" (o la insignia
    "Activa") y el botón "Desinstalar" (con confirmación; desactivado en la
    activa).
  - **Disponibles:** lista filtrada con "Instalar" y la barra de progreso
    (sondeo cada 1–2 s, como PHP), "Actualizar" en la mayor ya instalada y
    "Actualizar lista".
- Tests de vitest como `PHP.test.tsx`: se pinta la activa, activar llama a la
  ruta y las instaladas no muestran "Instalar".

## Pruebas y cierre

- Unit tests: el filtro de releases, el parseo de `SHASUMS256.txt` y de
  `settings.txt`, y la validación de `NodeVersion`.
- Integración (`tests/`): rutas con un `NodeManagerPort` falso sobre el temp dir
  y un `NodeReleasesPort` falso.
- Un test `#[ignore]` de red que descarga y verifica una versión real en un temp
  dir, como los de PHP y mkcert.
- Hecho = `cargo test`, `cargo clippy --all-targets` sin avisos, `npm test` y
  `npm run lint` en verde, y comprobación en la app de escritorio (instalar una
  versión, activarla, abrir una terminal nueva y que `node -v` coincida;
  `nvm list` la ve). Commit en su propia rama, sin push.

## Decisiones del implementador (marcadas como suyas)

- Junction en vez de symlink, para no necesitar administrador.
- Criterio de "línea mantenida" = última release de hace menos de 6 meses.
- Caché de 1 h para `index.json`.
- Desinstalar incluido (borra la carpeta de la versión, también en el almacén
  de nvm).
- Ignorar nvm que no estén detrás de `NVM_HOME` (como el de Laravel Herd).
