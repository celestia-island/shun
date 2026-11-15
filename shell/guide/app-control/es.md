# El control de aplicaciones de Windows bloqueó el inicio

La instalación terminó y los archivos están en su sitio. Lo que falló fue el inicio: Windows detuvo el ejecutable antes de arrancarlo y la aplicación informó **os error 4551**, el bloqueo que Windows llama **0x11C7**.

Windows solo ejecuta código que reconoce o firmado por un editor de confianza: el Control inteligente de aplicaciones en un Windows 11 personal, Control de aplicaciones (WDAC) o AppLocker en Windows 10 y en equipos administrados.

![Dónde está el interruptor](guide/app-control-1.png)

## Localizar el interruptor

- **①** Abre **Configuración → Privacidad y seguridad → Seguridad de Windows**, luego **Abrir Seguridad de Windows**.
- **②** Entra en **Control de aplicaciones y navegador** y después en **Control inteligente de aplicaciones**.
- **③** Lee el estado: **Activado** bloquea las aplicaciones sin firmar o desconocidas, **Evaluación** solo observa, **Desactivado** no aplica nada.

![Los tres estados del Control inteligente de aplicaciones](guide/app-control-2.png)

## La solución en tu propio equipo

Si el estado es **Activado**, la decisión es de este control: abre **Control inteligente de aplicaciones → Configuración** y ponlo en **Desactivado**. Para un usuario doméstico es la única palanca.

Microsoft describe este cambio como definitivo: recuperar la protección exigía antes restablecer o reinstalar Windows, aunque las actualizaciones recientes permiten reactivarla desde la aplicación Seguridad de Windows; la interfaz manda.

Si el estado es **Evaluación** o **Desactivado**, aquí no se bloquea nada; la política que actúa es otra, así que sigue el apartado para administradores.

![Tres caminos posibles](guide/app-control-3.png)

## Si el ajuste lo gestiona un administrador

En un PC de trabajo o de centro educativo el interruptor está en gris o no aparece, y Windows 10 no ofrece este control en absoluto. El bloqueo viene entonces de una directiva: envía a tu equipo de TI el código de error, la ruta del ejecutable y los detalles de eventos de abajo.

## Otras opciones

- **Pedir una compilación firmada.** La única vía que mantiene el control activo: Microsoft pide a los desarrolladores firmar con un certificado válido, y una compilación sin firmar sigue siendo no fiable se instale como se instale.
- **«Desbloquear» no es la solución.** Propiedades → Desbloquear solo quita la marca de «descargado de Internet»; no anula una directiva.
- **No existe una lista de permitidos por aplicación.** Microsoft indica que este control no se puede eludir para una sola aplicación.

## Comprobar las pruebas

Visor de eventos → Registros de aplicaciones y servicios → Microsoft → Windows → **CodeIntegrity → Operational**: **3077** es un bloqueo de una directiva aplicada, **3076** su versión en modo de auditoría. **3033** suele indicar una firma revocada o caducada, pero aparece también sin ninguna directiva de control: por sí solo no demuestra nada. Los bloqueos de scripts y MSI quedan en **AppLocker → MSI and Script**. ¿Ningún 3077? El bloqueo puede venir de otro sitio, o no haberse registrado.

## Páginas oficiales de Microsoft

[Preguntas frecuentes sobre el Control inteligente de aplicaciones](https://support.microsoft.com/es-es/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
[Control de aplicaciones para Windows](https://learn.microsoft.com/es-es/windows/security/application-security/application-control/app-control-for-business/appcontrol)

---

Corrige la causa, vuelve al instalador y pulsa de nuevo **Finalizar**; si ya lo cerraste, inicia la aplicación desde la carpeta donde la instalaste. Envía al desarrollador **0x11C7** y **os error 4551**, el estado del interruptor y el identificador de evento que hayas encontrado.
