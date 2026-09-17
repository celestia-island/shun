# Nota de diseño: entrega multiplataforma

Estado: **superficie de Windows verificada y sus huecos cerrados (2026-09);
backends de registro en runtime para Linux y macOS implementados; móvil
(Android/iOS) y HarmonyOS investigados — fuera de alcance por ahora.**
Esta nota registra (a) lo que una prueba concentrada de la superficie de
registro del autoempaquetado demostró sobre el backend de Windows enviado,
(b) las superficies antes ausentes — acceso directo de escritorio /
identidad de barra de tareas / menú contextual —, ahora implementadas, con
los hallazgos de política de seguridad en máquina real que las moldearon,
(c) los backends de Linux y macOS, y (d) el veredicto de viabilidad para
Android, iOS y HarmonyOS (aplazados).

El inventario ejecutable de (a) vive en
`tests/registration_shortcuts.rs` y `tests/msix_pack.rs`.

## 1. Lo que demostró la prueba concentrada de Windows (2026-09, máquina real)

Verificado ejecutando el `InstallFlow` real en una máquina Windows 11 y
leyendo los resultados a través del propio Windows (resolvedor COM
WScript.Shell, registro, MakeAppx del Windows SDK) — no inspeccionando
nuestra propia salida:

| Superficie | Resultado |
| --- | --- |
| El `.lnk` del menú Inicio resuelve a través del shell | **correcto** — `TargetPath` y `WorkingDirectory` vuelven exactamente como los declaró el contexto de instalación; el PIDL sintético construido a mano por `mslnk` resuelve correctamente |
| Rutas de instalación no ASCII (中文目录) | **correcto** — un directorio de instalación `顺测试目录` recorre el resolvedor COM de ida y vuelta byte-exacto |
| Binario `.lnk` contra MS-SHLLINK | **correcto** — cabecera, CLSID `{00021401-…}`, conjunto de flags (target ID list + relative path + working dir + unicode), sin hotkey |
| Entrada ARP (HKCU) | **correcto** — conjunto de campos completo, equivalente al de NSIS: DisplayName/Version/Publisher/InstallLocation/DisplayIcon más `UninstallString`/`ModifyPath`/`RepairString` (todos entre comillas, cada uno invocando la UI del desinstalador vía `/uninstall`) y `EstimatedSize` como DWORD; EstimatedSize coincide con el total del manifiesto del payload |
| Limpieza de la desinstalación | **correcto** — clave ARP, acceso directo, payload, desinstalador y directorio, todo eliminado (cubierto por `tests/install_local.rs`, reconfirmado aquí) |
| Generación del manifiesto MSIX | **correcto** — identidad, versión de cuatro partes con relleno, cadenas escapadas para XML, punto de entrada con barra diagonal, runFullTrust |
| Empaquetado MSIX real (MakeAppx 10.0.26100) | **correcto** — zip OPC válido con `[Content_Types].xml` + `AppxManifest.xml`; el `dist/shundemo-0.1.0-x64.msix` producido **no lleva bloque de firma** (por diseño: la distribución por Store lo firma, o hay que confiar en un certificado autofirmado) |

## 2. La pasada de cierre de huecos de Windows (implementada en 2026-09)

Todo lo que sigue aterrizó después de la pasada de verificación, con cada
superficie ejercitada por la suite de pruebas de registro en una máquina
real.

### Acceso directo de escritorio — `install.desktop-shortcut`

Resuelto desde la política (`always` | `never` | `ask`, la convención de
casilla de NSIS — el asistente egui muestra una casilla marcada por
defecto para `ask`, las ejecuciones headless responden marcada) y escrito
junto al del menú Inicio. El escritorio se resuelve mediante
**`SHGetKnownFolderPath(FOLDERID_Desktop)`** — nunca
`%USERPROFILE%\Desktop`, que es incorrecto siempre que el escritorio está
redirigido (OneDrive, políticas de dominio). La desinstalación lo elimina
incondicionalmente (la configuración puede haber cambiado entre la
instalación y la desinstalación).

**Medido en máquina real**: las políticas de seguridad (protección contra
atajos falsos y ransomware de AV/EDR) suelen denegar la creación de
`.lnk` en el escritorio *específicamente* — en la máquina de verificación
incluso `echo x > Desktop\probe.lnk` desde un shell elevado se deniega,
mientras que los archivos `.tmp` se escriben con libertad. Por eso el
acceso directo de escritorio es **de mejor esfuerzo**: una escritura
denegada se degrada a advertencia y nunca falla la instalación (el acceso
del menú Inicio y la entrada ARP son la superficie crítica). La suite de
pruebas sondea la política de la máquina y comprueba tanto el camino
feliz como el de la degradación elegante.

### Identidad de barra de tareas — `install.aumid`

El anclaje programático en la barra de tareas sigue **bloqueado por
diseño de la plataforma** (no existe API soportada; los hacks de anclaje
se eliminaron en Windows 10). Lo que se envió es la mitad de la
identidad: cada acceso directo queda sellado con un
**`System.AppUserModel.ID`** a través del almacén de propiedades COM del
Shell (`IShellLink` → `IPersistFile` → `IPropertyStore`, en
`src/targets/aumid.rs` — `mslnk` solo escribe bytes), de modo que la
agrupación en la barra de tareas, las jump lists y el anclaje *iniciado
por el usuario* se comportan bien. El AUMID por defecto se genera como
`{publisher}.{product}`; `install.aumid` lo anula, y la aplicación debe
pasar el mismo valor a `SetCurrentProcessExplicitAppUserModelID`. Las
instalaciones MSIX obtienen la identidad gratis a través del paquete. El
sellado es de mejor esfuerzo por la misma razón de política (la máquina
de verificación deniega `IPropertyStore::SetValue` sobre archivos
`.lnk` — 0x80030005 —, así que allí el sello se degrada a advertencia).

### Verbos del menú contextual — `[[install.verbs]]`

Nivel 1 enviado: verbos del Explorador por usuario bajo
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command` — la
superficie documentada de Application Registration, sin elevación,
visible sobre el exe de la aplicación y los accesos directos a él. Tres
destinos de verbo se mapean a líneas de comando en toda plataforma que
los implemente: `data-folder` (abre el directorio de instalación),
`uninstall` (ejecuta el desinstalador copiado), `app` (punto de entrada +
argumentos). La desinstalación elimina las claves de verbo que creó y
después los contenedores `shell`/`Applications` solo donde estén vacíos
(un verbo registrado por otro sobrevive). El nivel 2 (asociaciones de
tipo de archivo) y el nivel 3 (`FileExplorerExtension` de MSIX) siguen
siendo trabajo futuro.

### Enlaces profundos — `install.deep-links`

El modelo de entrega los prometió desde el primer borrador; ahora son
reales, en todos los backends, por usuario: Windows registra cada esquema
como clase de protocolo bajo `HKCU\Software\Classes\<scheme>` (el
marcador vacío `URL Protocol` + un comando open que recibe la URL como
`%1`); Linux declara `MimeType=x-scheme-handler/<scheme>;` en el lanzador
y reivindica el valor por defecto vía `xdg-mime`; un `Info.plist` de
macOS sintetizado lleva `CFBundleURLTypes`. Los esquemas se normalizan a
minúsculas `[a-z0-9+.-]` (`"MyApp://"` → `myapp`). La desinstalación
borra la clase de protocolo de Windows; eliminar el lanzador de Linux
deja huérfano al manejador (la línea de `mimeapps.list` queda inerte —
anotado, aceptado).

### Por qué el instalador no solicita elevación UAC

La pregunta surgió tras los hallazgos del acceso directo de escritorio,
y la respuesta tiene tres patas:

1. **Todo lo que escribe shun es superficie por usuario** — HKCU, el menú
   Inicio y el escritorio del usuario, `%LOCALAPPDATA%`. Nada de ello
   necesita un token elevado, así que un prompt UAC no compraría nada y
   solo añadiría el peor tipo de ruido de prompts (adiestrar a los
   usuarios a aceptar sin leer). Es el mismo intercambio que hace la
   plantilla NSIS de wowsp, y el mismo detrás de los setups de
   *usuario* de VS Code / Chrome.
2. **La elevación no arreglaría de todos modos el bloqueo de `.lnk`**:
   el bloqueo es el filtro de sistema de archivos de un producto de
   seguridad, claveado por la carpeta del escritorio y la extensión
   `.lnk` — los filtros interceptan procesos por política, no por ACL, y
   los procesos elevados también se filtran. (El caso de carpeta
   controlada de Windows se comporta igual: bloquea incluso a
   administradores salvo que la aplicación esté en la lista de
   permitidos.) La respuesta correcta es la que se envió: degradar a
   advertencia, mantener intactos el acceso del menú Inicio y el ARP.
3. **Las escrituras elevadas en superficies de *usuario* son una trampa
   de corrección**: un proceso elevado resuelve los perfiles de forma
   distinta (el escritorio de una cuenta de administrador, `%APPDATA%` y
   el hive del registro pueden diferir de los del usuario que instala) —
   el clásico bug de NSIS de los accesos para todos los usuarios.
   Cuando la elevación es realmente necesaria, el paso concreto se
   eleva *a sí mismo*: el bootstrapper Evergreen de WebView2 lleva su
   propio manifiesto `requireAdministrator`, así que el shell permanece
   `asInvoker` y delega.

Un ámbito para toda la máquina (`Program Files`, ARP en HKLM, accesos
para todos los usuarios) es un *modo* legítimo que algunos productos
necesitan — se envió exactamente como eso: un opt-in deliberado
(`install.scope`), nunca el valor por defecto. Véase la sección 6.

### Hallazgos de robustez, ambos corregidos

- Los nombres de producto con caracteres ilegales para nombres de
  archivo (`/\:*?"<>|`, puntos/espacios finales) se **sanitizan por
  stem** para toda superficie de sistema de archivos y registro (nombres
  `.lnk`, ruta de la clave ARP) — un `\` en un nombre de producto ya no
  anida subclaves de registro.
- El `UninstallString` del ARP pasa `/uninstall`, pero el analizador
  headless del shell de instalación solo aceptaba `--uninstall` — hacer
  clic en «Desinstalar» en Configuración de Windows lanzaba el asistente
  en lugar de desinstalar. Ambas grafías ahora se analizan.

## 3. Linux y macOS (backends de runtime implementados; las salidas de empaquetado, a continuación)

**Ambos son abordables, y ambos comparten con Windows un límite duro: el
anclaje programático de barra de tareas/dock no existe en ninguna
parte.** Los backends de `Registration` en runtime se enviaron; las
salidas de empaquetado del lado de construcción (deb/rpm vía
`tauri-bundler`, DMG) siguen siendo trabajo de seguimiento porque
necesitan sus máquinas de construcción nativas.

### Linux — `LinuxRegistration` (src/targets/freedesktop.rs)

Todo lo que hace el backend de Windows se mapea a convenciones
freedesktop, todo por usuario (`~/.local/share/...`), sin elevación:

- **registro del lanzador** = escribir `<product>.desktop` (Name, Exec,
  Icon desde `install.icon`, Categories y — lo crítico —
  **`StartupWMClass`** = el stem del ejecutable de entrada, el campo que
  hace que un anclaje de barra de tareas/dock iniciado por el usuario se
  agrupe bajo el icono correcto) en `~/.local/share/applications`, y
  después ejecutar `update-desktop-database` sobre él (es seguro
  omitirlo cuando la herramienta no está — los escritorios re-escanean
  de forma perezosa);
- **verbos del menú contextual + entrada de desinstalación** = `Actions=`
  + grupos `[Desktop Action <id>]` — incluyendo siempre una acción
  **Uninstall**, porque GNOME Software / KDE Discover solo listan las
  apps que rastrean sus propios backends de paquetes: una app instalada
  con shun jamás aparece allí. Los tres destinos de verbo se mapean a
  `xdg-open`, el desinstalador y el punto de entrada + argumentos;
- **restauración del bit de ejecución** — el archivo del payload lleva
  0644 en cada entrada, así que el backend devuelve con chmod el punto
  de entrada y el desinstalador copiado a 0755;
- **desregistro** = borrar el `.desktop` + refrescar la base de datos.

El escritor de `.desktop` es fontanería de datos pura que compila (y
está cubierta por pruebas unitarias) en todas las plataformas; solo la
mitad que lanza procesos está condicionada por la puerta de Linux. El
**anclaje en barra de tareas/dock sigue siendo imposible** (no existe API
multi-escritorio: los favoritos de GNOME son una clave gsettings interna,
los anclajes de KDE viven en un appletsrc sin documentar; trátelo como
acción del usuario). Los menús contextuales del gestor de archivos
(scripts de Nautilus / service menus de Dolphin) siguen fuera de
alcance.

**Formatos de empaquetado** (aún lado de construcción, pendientes):
**tarball/portable (shun ya lo tiene) + deb ([cargo-deb]) + rpm
([cargo-generate-rpm])** es el mejor subconjunto — exactamente lo que
emite `tauri-bundler` (es una biblioteca utilizable con payloads no
Tauri, al igual que [cargo-packager]). AppImage = medio; Flatpak =
medio-alto; snap = alto, aplazar. El límite honesto de «corre en
cualquier distro»: glibc solo es compatible hacia delante y **los
estáticos musl no pueden cargar aplicaciones WebView** (webkit2gtk
arrastra toda la pila C de GTK) — el shell de entrega puede ser
musl-static, pero las apps Tauri entregadas deben construirse contra la
línea base webkit2gtk-4.1 más antigua soportada (era Ubuntu 22.04 /
Debian 12 / Fedora 37+).

### macOS — `MacOSRegistration` (src/targets/macos.rs)

- **registro** = localizar el bundle `.app` en el que vive el ejecutable
  de entrada (el ancestro `.app` más cercano); sintetizar un
  `Info.plist` mínimo (`plist.rs`, puro y con pruebas unitarias en todas
  partes) cuando el payload no traía ninguno; restaurar el bit de
  ejecución del punto de entrada; **retirar el `com.apple.quarantine`
  heredado** de forma recursiva (los navegadores sellan el instalador,
  y las copias de macOS preservan los xattrs — sin esto, la app
  entregada hereda el bloqueo de Gatekeeper que el usuario ya había
  respondido); después `lsregister -f` el bundle — Spotlight y
  Launchpad siguen. Desregistro = `lsregister -u` (los archivos los
  elimina la pasada genérica de desinstalación). Los payloads de
  ejecutable desnudo (sin `.app`) se registran como no-op — convenciones
  portable;
- **anclaje en el Dock**: **sin API soportada** (el hack de `defaults
  write com.apple.dock` + `killall Dock` pisotea las preferencias del
  usuario y es poco fiable en macOS recientes) — la presencia en
  Launchpad/Spotlight vía registro LS es el equivalente en
  descubribilidad;
- **la firma sigue siendo trabajo de proceso obligatorio** para la
  distribución real: Developer ID + hardened runtime + `notarytool` +
  staple; y el shell descargado **será translocado** — maneje en
  consecuencia las suposiciones sobre la propia ruta;
- **salidas de empaquetado** (pendientes): DMG vía tauri-bundler /
  `hdiutil`, `.pkg` solo para flujos de administración, Homebrew cask
  como canal. Publique **universal2** (doble build + `lipo` +
  re-firma).

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android e iOS (investigados)

**Reencuadre primero: en móvil, la «instalación» es propiedad de la
plataforma y está verificada por firma. No existe el equivalente de
escribir en streaming un tar zstd de un directorio de aplicación en una
ubicación elegida por el usuario.** El papel móvil honesto de shun es un
**pipeline de construcción/empaquetado/firma** (un «cargo-dist para
móvil»), no un runtime de instalador.

### Android

- **Mecánica de empaquetado**: APK = zip firmado (DEX + recursos +
  `.so` por ABI); pipeline `aapt2` → `d8`/`r8` → package → `zipalign`
  → `apksigner`. AAB es el formato de publicación de Play (obligatorio
  para apps nuevas); la instalación lateral necesita un APK concreto
  (`bundletool build-apks` + re-firma). Los targets de Rust son Tier 2
  con herramientas de host.
- **Herramientas vivas (2025–2026)**: `tauri android build` de Tauri 2
  (envuelve cargo-mobile2 + Gradle; keystore vía
  `keystore.properties`), **cargo-ndk** (mantenido), el **crate
  `apk`** (aapt2/d8/zipalign/apksigner sin Gradle). **xbuild y
  cargo-apk están muertos/dormidos** — no construya sobre ellos.
  Tauri 2 móvil es oficialmente estable; el lado Android está más
  maduro que el de iOS.
- **Instalador en runtime: inviable/sin sentido.** El gestor de
  paquetes del SO instala desde un APK firmado con confirmación visible
  para el usuario (silencioso solo para device-owner/MDM); el APK *es*
  el instalador. La política de Play además **prohíbe la
  auto-actualización / la descarga de código ejecutable** — un runtime
  de actualización estilo shun sería una violación de política para
  cualquier app distribuida por Play. La instalación lateral se
  degrada por diseño: **la exigencia de instalación lateral de
  desarrollador verificado empieza el 2026-09-30** (las apps no
  verificadas pasan por un flujo de varios pasos con una espera de 24
  horas).
- **Accesos directos**: los iconos de la pantalla de inicio y los
  atajos de app (`shortcuts.xml`, `ShortcutManager`,
  `requestPinShortcut`) son **declarables solo por la app** — no existe
  API de inyección en tiempo de instalación. El análogo en tiempo de
  construcción es generar `shortcuts.xml`/intent-filters dentro del APK
  que shun empaqueta.

### iOS

- **Mecánica de empaquetado**: IPA = zip con `Payload/App.app` +
  `embedded.mobileprovision` (App ID + entitlements + certificado +
  lista de UDID permitidos Ad Hoc). Canales: App Store, TestFlight, Ad
  Hoc (100 dispositivos/tipo/año), Enterprise. **La cadena de
  herramientas de firma (codesign, xcodebuild, keychain) es
  exclusivamente macOS** — requisito duro de host.
- **Instalador en runtime: inviable** fuera de dos nichos: (a)
  manifiestos OTA Ad Hoc (`itms-services://?...manifest.plist`) —
  fácil, legal, nicho; (b) el régimen de Web Distribution / mercados
  alternativos de la UE — real, pero con puertas: elegibilidad de
  Apple + notarización + los términos de tarifa de oct 2026 (5% de
  Comisión de Tecnología Core), y solo en la UE. La instalación lateral
  con Apple ID gratuito (AltStore/Sideloadly) es un camino de
  aficionado de 7 días/3 apps, no un canal productizable.
- **Accesos directos**: no existe nada en tiempo de instalación, en
  ningún canal — los iconos de la pantalla de inicio, los esquemas de
  URL y los universal links los declara la app y los valida la firma.
- **fallback egui**: Android = `android-activity` + winit + wgpu
  (Vulkan/GLES); iOS = winit + wgpu (Metal) incrustado en un host UIKit
  vía FFI + proyecto Xcode. Ninguno tiene una historia llave en mano —
  un pipeline de empaquetado de shun es exactamente la pieza que
  falta.

## 5. HarmonyOS (investigado)

**Veredicto de entrada: en 2026, «shun soporta HarmonyOS» solo puede
significar honestamente una cosa — un objetivo de empaquetado/firma en
tiempo de construcción que produzca artefactos HAP/APP firmados para
release, más un flujo `hdc install` para dispositivos de desarrollo. Un
instalador en runtime (la mitad NSIS de shun) no tiene sustrato legal
ni técnico en HarmonyOS NEXT.**

Panorama (2025–2026): HarmonyOS NEXT (5.0, oct 2024) eliminó la capa de
compatibilidad APK; la línea a objetivo es HarmonyOS 6+ (API 20/23),
solo China, solo AppGallery, ~19% del mercado de SO de China.
OpenHarmony es la base abierta; HarmonyOS comercial es el producto de
Huawei encima — un empaquetador apunta al comercial.

- **Formato de paquete**: HAP (zip: `module.json5`, bytecode ArkTS,
  `libs/<abi>/*.so` nativas); paquetes compartidos HSP/HAR; `.app` =
  paquete de envío a AppGallery (`pack.info`). Las herramientas se usan
  por CLI: `ohpm` + `hvigorw assembleHap` + `hap-sign-tool` +
  `app_packing_tool.jar` (builds CI headless con soporte oficial).
- **Firma**: SHA256withECDSA; keystore `.p12` + `.cer` + perfil
  `.p7b` (nombre del bundle, permisos y, para debug, la lista de UDID
  permitidos del dispositivo); los certificados **los emite Huawei vía
  AppGallery Connect** (registro individual gratuito — sin cuota al
  estilo Apple).
- **Rust**: `aarch64/armv7/x86_64-unknown-linux-ohos` son **Tier 2 con
  herramientas de host** (listos por rustup desde 1.78). La cadena
  comunitaria `ohos.rs` (`cargo-ohos`, `napi-ohos`, `ohos-openssl`) es
  el pegamento; núcleo Rust + shell ArkTS es una arquitectura probada
  (RustDesk OHOS). **egui está bloqueado**: winit no tiene backend OHOS
  upstream (solo betas comunitarias). **Tauri**: una rama oficial pero
  sin fusionar, `feat/open-harmony` (parches wry/tao, CLI `cargo tauri
  ohos`), funciona hoy pero se mueve rápido — espere re-anclar
  versiones cada pocos meses.
- **Honestidad de la distribución**: la instalación lateral de consumo
  está efectivamente cerrada (solo AppGallery; `hdc install` necesita
  modo desarrollador + firma de Huawei + UDID). La publicación para
  dispositivos designados: 100 dispositivos/año, validez de 90 días.
  La distribución enterprise está hoy limitada a los PC empresariales
  Qingyun. **HarmonyOS PC** es real (ARM, distribución por tienda, aún
  sin instalación lateral) — Huawei ha *manifestado la intención* de
  abrir la instalación lateral en el PC más adelante; ese es el único
  punto a vigilar que algún día podría justificar allí un runtime de
  entrega de escritorio.

Tabla de esfuerzo: objetivo de empaquetado HAP **medio**; paso de
compilación cruzada de Rust **fácil–medio** (el wrapper clang del SDK
como linker, ohos-openssl para TLS); empaquetado Tauri-on-OHOS
**medio–difícil** (upstream sin fusionar); fallback egui **difícil**
(sin winit); instalador/flasheador en runtime **inviable**.

## 6. Ámbito de instalación y asistente declarativo (implementado en 2026-09)

### Ámbito de instalación — `install.scope = user | machine | ask`

Por usuario sigue siendo el valor por defecto (véase «Por qué el
instalador no solicita elevación UAC»). `machine` — o un `ask`
respondido con «todos los usuarios» — voltea cada superficie de registro
a su equivalente para toda la máquina: la entrada ARP bajo **HKLM**, el
acceso directo en el **menú Inicio de todos los usuarios**
(`%ProgramData%`), el acceso directo de escritorio en el **escritorio
público** (`FOLDERID_PublicDesktop`), verbos y enlaces profundos bajo
`HKLM\Software\Classes`. El shell detecta la resolución **antes** de que
el flujo se ejecute y, cuando aún no está elevado, se relanza a sí mismo
vía el verbo `runas` cargando las respuestas del usuario (`--silent
--mode=… --dir=… --scope=machine`): el consentimiento UAC es el único
prompt, mostrado solo para el modo que lo necesita — el patrón
bootstrapper, exactamente como se prometió. La desinstalación se refleja
(el `UninstallString` del ARP lanza el desinstalador, que se eleva de la
misma forma). El ámbito máquina es exclusivo de Windows; los backends de
Linux/macOS lo rechazan explícitamente. El test de integración se omite
salvo que el runner esté elevado (`cargo` desde un shell de
administrador lo ejercita).

### El pipeline del asistente — `[[package.metadata.shun.steps]]`

El asistente es ahora un pipeline declarativo, ordenado y de composición
libre, en lugar de una secuencia fija de modo → instalación. Cinco tipos
de paso:

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # modo de entrega + directorio; embebe las
                                 # casillas de `ask` (acceso de escritorio, ámbito)
[[package.metadata.shun.steps]]
kind = "scope"                   # elección usuario/máquina independiente
[[package.metadata.shun.steps]]
kind = "license"                 # panel de licencia (license / license-locales)
[[package.metadata.shun.steps]]
kind = "content"                 # panel markdown personalizado
title = "Release notes"
markdown = "notes.md"         # relativo al manifiesto
[[package.metadata.shun.steps]]
kind = "install"                 # la ejecución de entrega (exactamente uno obligatorio)
```

Sin `steps` = el pipeline por defecto (modo → licencia-si-se-declaró →
instalación) con los `custom-steps` legados inyectados tras sus claves
`after`; declarar ambos es un error de configuración, y también lo son
cero o varios pasos `install`. La pregunta de ámbito y la casilla del
acceso de escritorio aparecen donde las políticas `ask` se cruzan con un
panel: embebidas en el paso mode, o independientes (`scope`) — lo
compone el desarrollador. Los documentos de contenido y licencia se leen
**en tiempo de construcción** y se insertan en línea dentro de
`shun-steps.json` (`ShunConfig::resolve_steps`), así que los
instaladores de runtime no cargan dependencias de archivos. El fallback
egui renderiza el pipeline completo (riel por paso, puerta de licencia,
navegación atrás/siguiente); el `ShellView` de Tauri expone los pasos
resueltos para el front-end web.

## 7. Estado del portafolio (2026-09)

| Capacidad | Win | Linux | macOS | Android | iOS | HarmonyOS |
| --- | --- | --- | --- | --- | --- | --- |
| Instalación + registro en runtime | ✅ ámbito usuario + máquina | ✅ backend `.desktop` (usuario) | ✅ backend `.app` (usuario) | aplazado | aplazado | aplazado |
| Acceso de escritorio/menú Inicio | ✅ ambos (dirigidos por política) | ✅ entrada de lanzador | ✅ (Launchpad/Spotlight) | n/a | n/a | n/a |
| Anclaje en barra de tareas/dock | solo identidad (sello AUMID) | solo identidad (StartupWMClass) | solo identidad (LS) | n/a | n/a | n/a |
| Menú del clic derecho | ✅ verbos de nivel 1 | ✅ Desktop Actions | NSServices, más adelante | aplazado | aplazado | aplazado |
| Enlaces profundos | ✅ clase de protocolo | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | aplazado | aplazado | aplazado |
| Historia de desinstalación | ✅ ARP + auto-eliminación | ✅ basada en Action | ✅ desregistro LS + archivos | SO | SO | SO |
| Salida de empaquetado | ✅ instalador + MSIX | tarball ✅; deb/rpm a continuación | bundle ✅; DMG a continuación | aplazado | aplazado | aplazado |
| Realidad de la firma | Authenticode / Store | opcional | obligatoria (trabajo de proceso) | keystore / Play | certificados Apple | Huawei AGC |

**Aplazado por decisión (2026-09)**: Android, iOS y HarmonyOS — la
investigación de las secciones 4–5 sigue en pie; nada de ello está
programado.

**Siguiente en la cola**:

1. Salidas de empaquetado deb/rpm vía `tauri-bundler` (CI de Linux), DMG
   vía el carril de la máquina macOS.
2. Nivel 2 del menú contextual de Windows (asociaciones de tipo de
   archivo), si algún consumidor lo necesita.
3. **Nunca**: instaladores en runtime / inyección de accesos directos en
   cualquier SO de teléfono; anclaje de barra de tareas/dock en
   cualquier plataforma; snap hasta que su complejidad se lo gane.
