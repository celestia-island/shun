# Windows blocked the app from starting

The install worked and the files are there. Windows only blocked the start: the app reported **os error 4551**, which Windows also calls **0x11C7**.

![Where the switch is](guide/app-control-1.png)

## Turn it off

- **①** Open **Settings → Privacy & security → Windows Security**, then **Open Windows Security**.
- **②** Go to **App & browser control**, then **Smart App Control**.
- **③** If it says **On**, open **Smart App Control → Settings** and switch it to **Off**.

![The three states](guide/app-control-2.png)

That switch is awkward to undo: turning the protection back on used to mean resetting Windows, though recent updates can re-enable it from the app.

Greyed out, or no Smart App Control at all? Then an administrator or another policy owns it. Check **Event Viewer → Applications and Services Logs → Microsoft → Windows → CodeIntegrity → Operational**, event **3077**.

![Three ways out](guide/app-control-3.png)

[Microsoft's Smart App Control FAQ](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

Still blocked? Send the developer **0x11C7**, what the switch said, and that event ID.
