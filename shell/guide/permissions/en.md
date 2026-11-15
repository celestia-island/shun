# No permission to write there

The installer tried to write files into that folder and Windows said no. Usually the folder belongs to an administrator, sits under Program Files, or is a drive root.

- Pick a folder you own — the one the installer suggests is the safe default.
- Or close the installer, right-click it, and choose **Run as administrator**.
- On Linux, install into your home folder, not a system path — or run it with sudo.

The place the installer suggests by default sits in your user folder, so it needs no administrator rights at all. Folders on a USB stick or a network drive can also be refused — try a local disk first.

[Windows reference: User Account Control settings](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/settings-and-configuration)

---

Still stuck? Send the developer the folder you picked and the log line.
