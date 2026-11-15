# Windows 不让程序启动

安装是成功的，文件都在。只是第一次启动时被 Windows 拦住了：程序报 **os error 4551**，也就是 Windows 说的 **0x11C7**。

![开关在哪儿](guide/app-control-1.png)

## 把它关掉

- **①** 打开**设置 → 隐私和安全性 → Windows 安全中心**，点**打开 Windows 安全中心**。
- **②** 进入**应用和浏览器控制**，再点**智能应用控制**。
- **③** 如果显示的是**开**，点开**智能应用控制 → 设置**，改成**关**。

![三种状态](guide/app-control-2.png)

这个开关基本是单向的：想再把保护打开，以前要重置 Windows，近期的更新开始允许在应用里直接重新打开。

开关是灰的，或者根本没有智能应用控制？那是管理员或别的策略在管。到**事件查看器 → 应用程序和服务日志 → Microsoft → Windows → CodeIntegrity → Operational** 看事件 **3077**。

![三条路](guide/app-control-3.png)

[微软的智能应用控制问答](https://support.microsoft.com/zh-cn/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

还是不行？把 **0x11C7**、开关当时的状态和那个事件 ID 发给开发者。
