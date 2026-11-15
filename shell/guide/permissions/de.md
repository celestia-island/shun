# Keine Schreibrechte an diesem Ort

Der Installer wollte dort Dateien schreiben, Windows hat es abgelehnt. Meist gehört der Ordner einem Administrator, liegt unter Program Files oder ist der Anfang des Laufwerks.

- Wählen Sie einen Ordner, der Ihnen gehört — der vom Installer vorgeschlagene ist sicher.
- Oder schließen Sie den Installer, klicken Sie ihn mit rechts an und wählen **Als Administrator ausführen**.
- Unter Linux in den Home-Ordner installieren, nicht in einen Systempfad — sonst mit sudo starten.

Der vom Installer vorgeschlagene Ort liegt in Ihrem Benutzerordner — dort sind keine Administratorrechte nötig.

[Windows-Hilfe: So funktioniert die Benutzerkontensteuerung](https://learn.microsoft.com/de-de/windows/security/application-security/application-control/user-account-control/how-it-works)

---

Immer noch nicht? Schicken Sie dem Entwickler den Ordner und die Zeile aus dem Protokoll.
