# Windows hat den Start der App blockiert

Die Installation hat geklappt, die Dateien sind da. Blockiert hat Windows nur den Start: Die App meldet **os error 4551** — dasselbe, was Windows **0x11C7** nennt.

![Wo der Schalter ist](guide/app-control-1.png)

## Schalter ausschalten

- **①** Öffnen Sie **Einstellungen → Datenschutz und Sicherheit → Windows-Sicherheit**, dann **Windows-Sicherheit öffnen**.
- **②** Gehen Sie zu **App- und Browsersteuerung**, dann zu **Intelligente App-Steuerung**.
- **③** Steht er auf **Ein**, öffnen Sie **Intelligente App-Steuerung → Einstellungen** und stellen ihn auf **Aus**.

![Die drei Zustände](guide/app-control-2.png)

Der Schalter lässt sich schlecht zurückdrehen: Früher musste man Windows zurücksetzen, aktuelle Updates erlauben es direkt in der App.

Ausgegraut oder gar nicht vorhanden? Dann verwaltet ein Administrator oder eine andere Richtlinie das. Schauen Sie in die **Ereignisanzeige → Anwendungs- und Dienstprotokolle → Microsoft → Windows → CodeIntegrity → Operational**, Ereignis **3077**.

![Drei Wege](guide/app-control-3.png)

[Microsofts FAQ zur intelligenten App-Steuerung](https://support.microsoft.com/de-de/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

Immer noch blockiert? Schicken Sie dem Entwickler **0x11C7**, den Zustand des Schalters und diese Ereignis-ID.
