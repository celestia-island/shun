# The path is too long

Windows gives a path 260 characters by default. Your install folder plus its file names went past that, so Windows refused to create them.

- Pick a shorter folder, something like C:\App.
- Keep it near the drive root — every extra folder eats characters.

The log prints the full path it tried, so you can count the characters yourself.

[Microsoft on the limit: maximum path length](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation)

---

Still stuck? Send the developer the folder you picked.
