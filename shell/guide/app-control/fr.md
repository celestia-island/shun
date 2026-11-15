# Windows a bloqué le démarrage de l'application

L'installation a réussi, les fichiers sont là. C'est le démarrage que Windows a coupé : l'application a signalé **os error 4551**, que Windows appelle aussi **0x11C7**.

![Où est le commutateur](guide/app-control-1.png)

## Le désactiver

- **①** Ouvrez **Paramètres → Confidentialité et sécurité → Sécurité Windows**, puis **Ouvrir Sécurité Windows**.
- **②** Allez dans **Contrôle des applications et du navigateur**, puis **Contrôle intelligent des applications**.
- **③** S'il est sur **Activé**, ouvrez **Contrôle intelligent des applications → Paramètres** et passez-le sur **Désactivé**.

![Les trois états](guide/app-control-2.png)

Ce commutateur se remet mal : réactiver la protection demandait avant de réinitialiser Windows, mais les mises à jour récentes le permettent depuis l'application.

Grisé, ou absent ? Un administrateur ou une autre stratégie en a la charge. Regardez **Observateur d'événements → Journaux des applications et des services → Microsoft → Windows → CodeIntegrity → Operational**, événement **3077**.

![Trois pistes](guide/app-control-3.png)

[FAQ de Microsoft sur le contrôle intelligent](https://support.microsoft.com/fr-fr/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

Toujours bloqué ? Envoyez au développeur **0x11C7**, l'état du commutateur et cet identifiant d'événement.
