# Sin permiso para escribir ahí

El instalador intentó escribir en esa carpeta y Windows dijo que no. Suele pasar si la carpeta es de un administrador, está dentro de Program Files o es la raíz del disco.

- Elige una carpeta tuya: la que propone el instalador por defecto es segura.
- O cierra el instalador, haz clic derecho y elige **Ejecutar como administrador**.
- En Linux instala en tu carpeta personal, no en una ruta del sistema; si no, ejecútalo con sudo.

La ubicación que propone el instalador está en tu carpeta de usuario, así que no necesita permisos de administrador. Una carpeta en un USB también puede rechazarse.

[Referencia de Windows: configuración del Control de cuentas de usuario](https://learn.microsoft.com/es-es/windows/security/application-security/application-control/user-account-control/settings-and-configuration)

---

¿Sigue igual? Envía al desarrollador la carpeta elegida y la línea del registro.
