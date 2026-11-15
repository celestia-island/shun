# Windows bloqueó el arranque de la aplicación

La instalación salió bien y los archivos están. Windows cortó el arranque: la aplicación informó **os error 4551**, que Windows también llama **0x11C7**.

![Dónde está el interruptor](guide/app-control-1.png)

## Desactívalo

- **①** Abre **Configuración → Privacidad y seguridad → Seguridad de Windows** y pulsa **Abrir Seguridad de Windows**.
- **②** Entra en **Control de aplicaciones y navegador** y luego en **Control inteligente de aplicaciones**.
- **③** Si pone **Activado**, abre **Control inteligente de aplicaciones → Configuración** y ponlo en **Desactivado**.

![Los tres estados](guide/app-control-2.png)

Este interruptor cuesta volver a ponerlo: reactivar la protección obligaba antes a restablecer Windows, aunque las actualizaciones recientes ya lo permiten desde la aplicación.

¿Está en gris o no existe? Entonces lo gestiona un administrador u otra directiva. Mira **Visor de eventos → Registros de aplicaciones y servicios → Microsoft → Windows → CodeIntegrity → Operational**, evento **3077**.

![Tres caminos](guide/app-control-3.png)

[Preguntas frecuentes de Microsoft](https://support.microsoft.com/es-es/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

¿Sigue bloqueado? Envía al desarrollador **0x11C7**, el estado del interruptor y ese identificador de evento.
