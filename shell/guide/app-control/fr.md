# Lancement bloqué par le contrôle des applications Windows

L'installation est terminée, les fichiers sont en place. C'est le lancement qui a échoué : Windows a arrêté l'exécutable avant son démarrage, et l'application a signalé **os error 4551** — le blocage que Windows nomme **0x11C7**.

Windows n'exécute que du code qu'il reconnaît ou signé par un éditeur de confiance : le contrôle intelligent des applications sur un Windows 11 personnel, App Control for Business ou AppLocker sous Windows 10 et sur un PC géré.

![Où se trouve le commutateur](guide/app-control-1.png)

## Trouver le commutateur

- **①** Ouvrez **Paramètres → Confidentialité et sécurité → Sécurité Windows**, puis **Ouvrir Sécurité Windows**.
- **②** Choisissez **Contrôle des applications et du navigateur**, puis **Contrôle intelligent des applications**.
- **③** Lisez l'état : **Activé** bloque les applications non signées ou inconnues, **Évaluation** observe sans bloquer, **Désactivé** n'applique rien.

![Les trois états du contrôle intelligent des applications](guide/app-control-2.png)

## La solution sur votre propre PC

Si l'état est **Activé**, c'est ce contrôle qui a tranché : ouvrez **Contrôle intelligent des applications → Paramètres** et passez-le sur **Désactivé**. Pour un particulier, c'est le seul levier.

Microsoft décrit cette manipulation comme définitive : réactiver la protection exigeait auparavant de réinitialiser ou de réinstaller Windows, mais des mises à jour récentes le permettent depuis l'application Sécurité Windows ; c'est l'interface qui fait foi.

Si l'état est **Évaluation** ou **Désactivé**, rien n'est bloqué de ce côté — une autre stratégie s'applique, voir la section administrateur.

![Trois pistes possibles](guide/app-control-3.png)

## Si un administrateur gère ce réglage

Sur un PC professionnel ou scolaire, le commutateur est grisé ou absent, et Windows 10 ne propose pas ce contrôle du tout. Le blocage vient alors d'une stratégie : envoyez à votre service informatique le code d'erreur, le chemin de l'exécutable et les détails d'événement ci-dessous.

## Autres options

- **Demander une version signée.** La seule solution qui préserve le contrôle : Microsoft demande aux développeurs de signer avec un certificat valide, et une version non signée reste non fiable quelle que soit l'installation.
- **« Débloquer » n'est pas une solution.** Propriétés → Débloquer retire seulement la marque « téléchargé depuis Internet » ; cela ne passe pas outre une stratégie.
- **Aucune liste d'autorisation par application.** Microsoft indique que ce contrôle ne peut pas être contourné pour une seule application.

## Vérifier les traces

Observateur d'événements → Journaux des applications et des services → Microsoft → Windows → **CodeIntegrity → Operational** : **3077** est un blocage par une stratégie appliquée, **3076** sa version en mode audit. **3033** signale souvent une signature révoquée ou expirée, mais apparaît aussi sans aucune stratégie de contrôle : seul, il ne prouve rien. Les blocages de scripts et de MSI figurent dans **AppLocker → MSI and Script**. Aucun 3077 ? Le blocage peut venir d'ailleurs, ou n'avoir pas été journalisé.

## Pages officielles de Microsoft

[Questions fréquentes sur le contrôle intelligent des applications](https://support.microsoft.com/fr-fr/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
[Contrôle d'application pour Windows](https://learn.microsoft.com/fr-fr/windows/security/application-security/application-control/app-control-for-business/appcontrol)

---

Corrigez la cause, puis revenez dans l'installateur et cliquez de nouveau sur **Terminer** ; si l'installateur est déjà fermé, lancez l'application depuis son dossier d'installation. Transmettez au développeur **0x11C7** et **os error 4551**, l'état du commutateur et l'identifiant d'événement trouvé.
