# 沒有權限寫到那裡

安裝程式想往這個資料夾寫檔案，Windows 說不行。多半是資料夾歸系統管理員所有、放在 Program Files 下面，或者是磁碟根目錄。

- 換一個你自己的資料夾——安裝程式預設給的那個位置就是安全的。
- 或者關掉安裝程式，按右鍵，選**以系統管理員身分執行**。
- Linux 上裝到家目錄裡，別裝進系統目錄；一定要裝系統目錄就用 sudo。
- 別裝到 C:\ 根目錄或 Program Files，那些地方一律要系統管理員權限。

安裝程式預設給你的位置在使用者目錄下，本來就不需要系統管理員權限，換成它就繞開了這件事。USB 隨身碟或網路磁碟上的資料夾也可能被拒，先換回本機磁碟再試。

裝到 C:\Program Files 或系統碟根目錄，安裝程式一定會被拒絕，那些地方只有系統管理員能寫。

[Windows 參考：使用者帳戶控制怎麼運作](https://learn.microsoft.com/zh-tw/windows/security/application-security/application-control/user-account-control/how-it-works)

---

還是不行？把你選的資料夾和日誌裡那行錯誤發給開發者。
