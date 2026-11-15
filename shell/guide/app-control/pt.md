# O Windows bloqueou o arranque da aplicação

A instalação correu bem e os ficheiros estão lá. O Windows cortou o arranque: a aplicação comunicou **os error 4551**, o mesmo que o Windows chama **0x11C7**.

![Onde está o interruptor](guide/app-control-1.png)

## Desligue-o

- **①** Abra **Definições → Privacidade e segurança → Segurança do Windows** e depois **Abrir a Segurança do Windows**.
- **②** Entre em **Controlo de aplicações e do navegador** e depois em **Controlo Inteligente de Aplicações**.
- **③** Se estiver em **Ativado**, abra **Controlo Inteligente de Aplicações → Definições** e mude para **Desativado**.

![Os três estados](guide/app-control-2.png)

Este interruptor custa a repor: reativar a proteção obrigava antes a repor o Windows, mas atualizações recentes já o permitem a partir da aplicação.

Esbatido ou inexistente? Então é um administrador ou outra política que manda. Veja o **Visualizador de Eventos → Registos de Aplicações e Serviços → Microsoft → Windows → CodeIntegrity → Operational**, evento **3077**.

![Três caminhos](guide/app-control-3.png)

[Perguntas frequentes da Microsoft](https://support.microsoft.com/pt-pt/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

Continua bloqueado? Envie ao programador **0x11C7**, o estado do interruptor e esse ID de evento.
