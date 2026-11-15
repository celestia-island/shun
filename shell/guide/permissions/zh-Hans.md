# 没权限写到那儿

安装器想往这个文件夹写文件，Windows 说不行。多半是文件夹归管理员所有、放在 Program Files 下面，或者是磁盘根目录。

- 换一个你自己的文件夹——安装器默认给的那个位置就是安全的。
- 或者关掉安装器，右键它，选**以管理员身份运行**。
- Linux 上装到家目录里，别装进系统目录；一定要装系统目录就用 sudo。
- 别装到 C:\ 根目录或者 Program Files，那些地方一律要管理员权限。

安装器默认给你的位置在用户目录下，本来就不需要管理员权限，换成它就绕开了这件事。U 盘或网络盘上的文件夹也可能被拒，先换回本机磁盘再试。

装到 C:\Program Files 或者系统盘根目录，安装器一定会被拒，那些地方只有管理员能写。

[Windows 参考：用户账户控制的设置](https://learn.microsoft.com/zh-cn/windows/security/application-security/application-control/user-account-control/settings-and-configuration)

---

还是不行？把你选的文件夹和日志里那行错误发给开发者。
