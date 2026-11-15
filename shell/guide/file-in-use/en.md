# A file is still in use

The installer has to replace a file that something else is holding open. Usually it's the app itself — still running, or only just closed.

- Quit the app (check the tray too), wait a few seconds, then run the installer again.
- If that doesn't help, restart the computer. A reboot always releases the file.

The install log names the file it could not replace, so that line tells you what to close. A leftover process often hides in the notification area, so quit it there too.

[Windows reference: fixing a sharing violation](https://learn.microsoft.com/en-us/troubleshoot/windows-server/performance/copying-exe-files-sharing-violation-error-folder-in-use)

---

Still stuck? Send the developer the file name from the log.
