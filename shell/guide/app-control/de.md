# Start durch Windows-App-Steuerung blockiert

Die Installation ist durchgelaufen, die Dateien liegen auf der Platte. Gescheitert ist der Start: Windows hat die Anwendung vor dem Start gestoppt, und sie meldet **os error 4551** — dieselbe Sperre, die Windows **0x11C7** nennt.

Windows führt nur Code aus, den es erkennt oder der von einem vertrauenswürdigen Herausgeber signiert ist: die intelligente App-Steuerung auf einem privaten Windows 11, App Control for Business oder AppLocker unter Windows 10 und auf verwalteten PCs.

![Wo der Schalter sitzt](guide/app-control-1.png)

## Den Schalter finden

- **①** Öffnen Sie **Einstellungen → Datenschutz und Sicherheit → Windows-Sicherheit**, dann **Windows-Sicherheit öffnen**.
- **②** Gehen Sie zu **App- und Browsersteuerung** und dann zu **Intelligente App-Steuerung**.
- **③** Lesen Sie den Zustand: **Ein** blockiert nicht signierte oder unbekannte Apps, **Auswertung** beobachtet nur, **Aus** setzt nichts durch.

![Die drei Zustände der intelligenten App-Steuerung](guide/app-control-2.png)

## Die Lösung auf dem eigenen PC

Steht der Schalter auf **Ein**, hat die intelligente App-Steuerung entschieden: Öffnen Sie **Intelligente App-Steuerung → Einstellungen** und stellen Sie sie auf **Aus**. Für private Nutzer ist das der einzige Hebel.

Microsoft beschreibt diesen Schritt als Einbahnstraße: Früher war für die Rückkehr des Schutzes ein Zurücksetzen oder Neuinstallieren nötig; aktuelle Updates erlauben das wieder in der Windows-Sicherheit-App, maßgeblich ist die Oberfläche.

Steht der Schalter auf **Auswertung** oder **Aus**, blockiert hier nichts — dann greift eine andere Richtlinie, siehe den Abschnitt für Administratoren.

![Drei Wege nach vorn](guide/app-control-3.png)

## Wenn ein Administrator die Einstellung verwaltet

Auf einem Arbeits- oder Schul-PC ist der Schalter ausgegraut oder fehlt, und Windows 10 kennt diese Steuerung gar nicht. Dann kommt die Sperre von einer Richtlinie: Schicken Sie Ihrer IT den Fehlercode, den Pfad der Anwendung und die Ereignisdetails unten.

## Weitere Möglichkeiten

- **Eine signierte Version anfordern.** Der einzige Weg, der die App-Steuerung aktiv lässt: Microsoft rät Entwicklern, mit einem gültigen Zertifikat zu signieren, und eine unsignierte Version bleibt nicht vertrauenswürdig, egal wie sie installiert wurde.
- **„Zulassen“ in den Dateieigenschaften hilft nicht.** Eigenschaften → Zulassen entfernt nur die Markierung „aus dem Internet geladen“.
- **Keine Freigabe pro App.** Microsoft stellt klar, dass diese Steuerung nicht für eine einzelne App umgangen werden kann.

## Belege prüfen

Ereignisanzeige → Anwendungs- und Dienstprotokolle → Microsoft → Windows → **CodeIntegrity → Operational**: **3077** ist eine Sperre durch eine durchgesetzte Richtlinie, **3076** dieselbe im Auditmodus. **3033** deutet oft auf eine widerrufene oder abgelaufene Signatur, tritt aber auch ohne jede App-Control-Richtlinie auf und beweist allein nichts. Sperren von Skripten und MSI stehen in **AppLocker → MSI and Script**. Gar kein 3077? Dann kam die Sperre vielleicht woanders her — oder wurde nicht protokolliert.

## Offizielle Microsoft-Seiten

[Häufig gestellte Fragen zur intelligenten App-Steuerung](https://support.microsoft.com/de-de/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
[App Control für Windows](https://learn.microsoft.com/de-de/windows/security/application-security/application-control/app-control-for-business/appcontrol)

---

Beheben Sie die Ursache, wechseln Sie dann zurück in den Installer und klicken Sie erneut auf **Fertigstellen**; ist der Installer schon geschlossen, starten Sie die Anwendung aus ihrem Installationsordner. Schicken Sie dem Entwickler **0x11C7** und **os error 4551**, den Zustand des Schalters und die Ereignis-ID.
