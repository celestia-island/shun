# Windows 不讓程式啟動

安裝是成功的，檔案都在。只是第一次啟動時被 Windows 擋住了：程式回報 **os error 4551**，也就是 Windows 說的 **0x11C7**。

![開關在哪裡](guide/app-control-1.png)

## 把它關掉

- **①** 開啟**設定 → 隱私權與安全性 → Windows 安全性**，點**開啟 Windows 安全性**。
- **②** 進入**應用程式與瀏覽器控制**，再點**智慧型應用程式控制**。
- **③** 如果顯示的是**開啟**，點開**智慧型應用程式控制 → 設定**，改成**關閉**。

![三種狀態](guide/app-control-2.png)

這個開關基本上是單向的：想再把保護打開，以前要重設 Windows，近期的更新開始允許在應用程式裡直接重新開啟。

開關是灰的，或者根本沒有智慧型應用程式控制？那是系統管理員或別的策略在管。到**事件檢視器 → 應用程式與服務記錄 → Microsoft → Windows → CodeIntegrity → Operational** 看事件 **3077**。

![三條路](guide/app-control-3.png)

[微軟的智慧型應用程式控制問答](https://support.microsoft.com/zh-tw/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

還是不行？把 **0x11C7**、開關當下的狀態和那個事件 ID 發給開發者。
