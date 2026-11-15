# Arranque bloqueado pelo controlo de aplicações do Windows

A instalação terminou e os ficheiros estão no sítio. O que falhou foi o arranque: o Windows parou o executável antes de o iniciar e a aplicação comunicou **os error 4551** — o bloqueio que o Windows chama **0x11C7**.

O Windows só executa código que reconhece ou assinado por um editor de confiança: o Controlo Inteligente de Aplicações num Windows 11 pessoal, o Controlo de Aplicações (WDAC) ou o AppLocker no Windows 10 e em PCs geridos.

![Onde está o interruptor](guide/app-control-1.png)

## Encontrar o interruptor

- **①** Abra **Definições → Privacidade e segurança → Segurança do Windows** e depois **Abrir a Segurança do Windows**.
- **②** Entre em **Controlo de aplicações e do navegador** e depois em **Controlo Inteligente de Aplicações**.
- **③** Leia o estado: **Ativado** bloqueia aplicações não assinadas ou desconhecidas, **Avaliação** apenas observa, **Desativado** não aplica nada.

![Os três estados do Controlo Inteligente de Aplicações](guide/app-control-2.png)

## A solução no seu próprio PC

Se o estado for **Ativado**, a decisão é deste controlo: abra **Controlo Inteligente de Aplicações → Definições** e mude para **Desativado**. Para um utilizador doméstico é a única alavanca.

A Microsoft descreve este passo como definitivo: recuperar a proteção exigia antes repor ou reinstalar o Windows, mas atualizações recentes já permitem reativá-la na aplicação Segurança do Windows; a interface é que manda.

Se o estado for **Avaliação** ou **Desativado**, aqui não se bloqueia nada — a política em vigor é outra, siga a secção para administradores.

![Três caminhos possíveis](guide/app-control-3.png)

## Se a definição for gerida por um administrador

Num PC de trabalho ou de escola o interruptor está esbatido ou ausente, e o Windows 10 não tem este controlo. O bloqueio vem então de uma política: envie à sua equipa de TI o código de erro, o caminho do executável e os detalhes de eventos abaixo.

## Outras opções

- **Pedir uma compilação assinada.** A única via que mantém o controlo ativo: a Microsoft pede aos programadores que assinem com um certificado válido, e uma compilação não assinada continua a não ser fidedigna, seja qual for a instalação.
- **«Desbloquear» não resolve.** Propriedades → Desbloquear apenas retira a marca de «transferido da Internet».
- **Não há lista de permitidos por aplicação.** A Microsoft indica que este controlo não pode ser contornado para uma única aplicação.

## Confirmar as provas

Visualizador de Eventos → Registos de Aplicações e Serviços → Microsoft → Windows → **CodeIntegrity → Operational**: **3077** é um bloqueio por uma política aplicada, **3076** a versão em modo de auditoria. **3033** costuma indicar uma assinatura revogada ou expirada, mas também surge sem qualquer política de controlo: por si só não prova nada. Os bloqueios de scripts e MSI ficam em **AppLocker → MSI and Script**. Sem nenhum 3077, o bloqueio pode vir de outro sítio — ou não ter ficado registado.

## Páginas oficiais da Microsoft

[Perguntas frequentes sobre o Controlo Inteligente de Aplicações](https://support.microsoft.com/pt-pt/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
[Controlo de Aplicações para Windows](https://learn.microsoft.com/pt-pt/windows/security/application-security/application-control/app-control-for-business/appcontrol)

---

Corrija a causa, volte ao instalador e clique novamente em **Concluir**; se já o fechou, inicie a aplicação a partir da pasta onde a instalou. Envie ao programador **0x11C7** e **os error 4551**, o estado do interruptor e o ID de evento que encontrou.
