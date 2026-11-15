# Launch blocked by Windows app control

The installation finished; the launch did not. Windows stopped the executable before it ran and the app reported **os error 4551** — the block Windows calls **0x11C7**. The files are on disk; only starting them was refused.

Windows runs only code it recognises, or code signed by a trusted publisher. At home on Windows 11 that filter is Smart App Control; on Windows 10 or a managed PC it is App Control for Business or AppLocker.

![Where the Smart App Control switch sits](guide/app-control-1.png)

## Find the switch

- **①** Open **Settings → Privacy & security → Windows Security**, then **Open Windows Security**.
- **②** Choose **App & browser control**, then **Smart App Control**.
- **③** Read the state: **On** blocks unsigned or unknown apps, **Evaluation** only observes, **Off** enforces nothing.

![The three Smart App Control states](guide/app-control-2.png)

## The fix on your own PC

If the switch reads **On**, Smart App Control made this call: open **Smart App Control → Settings** and set it to **Off**. For a home user that is the only lever.

Microsoft documents this as a one-way move: re-enabling it historically required resetting or reinstalling Windows, though recent updates can re-enable it inside the Windows Security app — that app is the authority.

If it reads **Evaluation** or **Off**, Smart App Control blocks nothing; another policy is enforcing — use the administrator branch.

![Three ways forward](guide/app-control-3.png)

## If an administrator owns the setting

On a work or school PC the toggle is greyed out or missing, and Windows 10 has no Smart App Control at all. The block then comes from a policy — send IT the error code, the executable's path and the event details below.

## Other options

- **Ask for a signed build.** The only fix that keeps app control on: Microsoft asks developers to sign with a valid certificate, and unsigned builds stay untrusted.
- **Unblocking the file is no fix.** Properties → Unblock only clears the download mark; it does not override a policy.
- **No per-app allow list exists.** Microsoft states Smart App Control cannot be bypassed for a single app.

## Check the evidence

Event Viewer → Applications and Services Logs → Microsoft → Windows → **CodeIntegrity → Operational**: **3077** is a block by an enforced policy and **3076** its audit twin. **3033** often means a revoked or expired signature, but it can appear with no App Control policy at all. Script and MSI blocks sit in **AppLocker → MSI and Script**. No 3077 at all? The block may have come from elsewhere, or was simply not logged.

## Microsoft's own pages

[Smart App Control: frequently asked questions](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)
[Application Control for Windows](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/appcontrol)

---

Fix the cause, then return to the installer and click **Finish** again; if the installer is already closed, start the program from its install folder. Send the developer **0x11C7** with **os error 4551**, the switch state and the event ID.
