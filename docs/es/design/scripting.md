# Nota de diseño: scripting en tiempo de instalación

Estado: **runner decidido — duckscript. Python investigado y demostrado
incrustable; adopción pendiente.** duckscript es el único runner de
scripting; la opción de JavaScript queda descartada (el juego de
herramientas cargo-make alrededor de duckscript está completo, y donde no
lo está, llamar a Python es la válvula de escape). Esta nota registra la
decisión y la investigación de incrustación de Python.

## duckscript (el runner)

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk) (Apache-2.0, el
lenguaje de scripting de cargo-make) se incrusta como una dependencia
ordinaria: cargar el conjunto de comandos en un `Context`, registrar las
integraciones de shun como comandos personalizados, ejecutar scripts con
control de flujo y std fs/env/net. La prueba de viabilidad vive en
`tests/scripting_duckscript.rs`. justfile en sí no es incrustable (el
crate `just` es una CLI, sin API de biblioteca estable) — duckscript es
el miembro incrustable de esa familia.

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # empaquetado por `shun build`
```

La superficie integrada de shun se registra como comandos duckscript:
`shun_progress`, `shun_emit`, `shun_fetch` (descargas verificadas), más
los comandos std del propio SDK (fs, env, http, process, semver, ...).

Puntos delicados a normalizar en los wrappers de shun: las rutas con
barra invertida de Windows son caracteres de escape en los argumentos de
duckscript (pase rutas con barra diagonal), y la asignación es sintaxis
de captura de salida (`x = cmd args`).

## Python incrustado — medido (sonda ejecutada 2026-09)

Todo lo que sigue se ejecutó de verdad, dos veces: sobre un CPython
3.13.5 del host y sobre un **runtime embeddable transportado**
(`python-3.13.5-embed-amd64.zip` desempaquetado, con el ejemplo
`pyembed_runner` de PyO3 colocado al lado para que `python313.dll` y la
stdlib carguen desde la carpeta transportada — `sys.prefix` confirmó el
directorio transportado):

| Capacidad | Resultado |
| --- | --- |
| HTTPS real (urllib + TLS) | correcto (pypi.org directo estaba bloqueado por la red localmente; example.com/el espejo de tencent, bien) |
| SHA-256 + HMAC en streaming | correcto |
| Ida y vuelta AES-CTR, firmar/verificar RSA-2048 | correcto — vía el wheel `cryptography` preinstalado en el runtime transportado (`pip --target runtime/Lib/site-packages` + habilitar `import site` en `python313._pth`) |
| Identidad de máquina | MachineGuid (winreg), MAC (`uuid.getnode`), número de serie del volumen C: (ctypes `GetVolumeInformationW`) — todo correcto |
| TPM | `tbs.dll` vía ctypes se alcanzó correctamente; el firmware de la máquina de la sonda tiene el TPM deshabilitado, así que `Tbsi_Context_Create` devuelve `TBS_E_TPM_NOT_FOUND` (0x8028400F — nota: NO 0x80284002, que es `TBS_E_BAD_PARAMETER` por una struct de parámetros NULL). La ruta de llamada está validada; en hardware con TPM habilitado, el mismo código lee `TPM_PT_MANUFACTURER` |

Tamaños medidos: zip embeddable **10.9 MB** / desempaquetado **20.4 MB** /
+wheel de cryptography **32.4 MB**. El binario runner de PyO3 en sí ocupa
~0.2 MB. Los wheels de terceros con `.pyd` nativos (como cryptography)
funcionan sin cambios — envíelos dentro del runtime transportado.

Puntos delicados registrados para la integración real: el intérprete
incrustado no se finaliza al hacer drop — haga flush explícito de stdio
después de ejecutar los scripts (véase el ejemplo runner); `eval` solo
acepta expresiones; pip contra el runtime transportado necesita
`--target` más el ajuste del `._pth` (o un runtime
python-build-standalone, que trae pip).

## Incrustación del WebView2 de versión fija — medido

Pregunta: ¿puede el instalador transportar el motor WebView2 en sí
mismo, alimentando a la vez su propia UI y la app desplegada?
**Mecánicamente sí — probado de punta a punta**; el costo está en el
payload.

- cab de versión fija v151.0.4129.101 x64: **307,241,094 bytes ≈ 293 MB**
  comprimido, **661.1 MB desempaquetado**.
- El shell de demo corrió contra el runtime transportado desempaquetado
  (`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`, ya la primera sonda en
  `webview2_available`): la UI renderizó (verificado con captura
  offline) y **los seis procesos de renderizado vinieron de la carpeta
  transportada**, no del Evergreen del sistema.
- Veredicto: viable; el costo de ~300 MB está **aceptado** (en línea con
  otros empaquetadores). La preocupación por la doble copia está
  eliminada por diseño: el artefacto incrusta UNA copia — el shell se
  autoarranca desde el subárbol de runtime del payload (staging con
  `extract_prefix` + reutilización consciente del hash en la
  extracción), así el instalador y la app instalada lo comparten;
  véase la sección de estrategias WebView2 en configuration.md. El
  fallback egui sigue siendo el suelo de costo cero para máquinas sin
  nada en absoluto.


## Python incrustado (investigado, viable)

**Veredicto: sí — Rust puede incrustar un CPython pequeño, y de forma
limpia.** La prueba es `tests/scripting_python.rs`, tras la feature
opcional `python-probe`: [PyO3](https://pyo3.rs) con `auto-initialize`
incrusta el intérprete en el proceso, evalúa Python real con la stdlib,
llama a funciones Rust personalizadas y mapea las excepciones de Python
a errores de Rust. La feature jamás entra en la build por defecto; en
CI solo la pata de Windows (que resuelve `--all-features` contra un
CPython preinstalado) la ejercita.

Opciones de transporte para un instalador autocontenido, a imagen de la
tabla de estrategias WebView2:

| Opción | Transporta | Notas |
| --- | --- | --- |
| `system` (por defecto) | nada | los comandos `process` de duckscript pueden invocar un python instalado; se degrada con elegancia cuando no existe |
| `embeddable` | paquete embeddable de Windows (~12–16 MB) | `python-3.x.x-embed-amd64.zip` oficial: `python3xx.dll` + zip de stdlib + `._pth`, sin admin, sin registro — un runtime privado exactamente igual que el WebView2 de versión fija |
| `standalone` | python-build-standalone (~30–60 MB) | distribuciones [custodiadas por Astral](https://astral.sh/blog/python-build-standalone) (lo que envía `uv`); multiplataforma, con versión fijada, completas; desproporcionado salvo que se necesiten pip/dependencias nativas |

Boceto:

```toml
[package.metadata.shun.script.python]    # válvula de escape pesada, opcional
type = "embeddable"                      # system | embeddable | standalone
```

Alternativas rechazadas/aplazadas:

- **RustPython** (MIT, Rust puro) — se declara a sí mismo no listo para
  producción, con huecos en la stdlib y sin módulos de extensión C;
  atractivo algún día, hoy no para instaladores.
- **PyOxidizer / `pyembed`** — incrustación de más alto nivel, pero el
  proyecto está en modo mantenimiento; aquí PyO3 por sí solo es
  suficiente.

Preguntas abiertas antes de conectarlo:

- Presupuesto de tamaño: ¿es aceptable +12–16 MB en el artefacto para
  los productos que lo habilitan? (La incrustación es opcional por
  manifiesto, así que el artefacto por defecto sigue siendo pequeño.)
- Acoplamiento de versiones: PyO3 enlaza el CPython del host de
  construcción; el runtime enviado debe coincidir. Fíjelo construyendo
  contra la misma distribución que se envía (`PYO3_PYTHON` →
  directorio embeddable/standalone desempaquetado).
- Aislamiento: intérprete incrustado en el proceso (UX de instalador
  monofichero) frente a subproceso (aislamiento de crashes más
  simple) — o ambos, elegido por hook.
- Qué hooks pueden escalar a Python en absoluto (¿solo prepare, o
  también las rutas de reparación post-instalación?).
