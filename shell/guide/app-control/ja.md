# Windows がアプリの起動を止めました

インストールは成功していて、ファイルもそろっています。止められたのは起動だけです。プログラムは **os error 4551** を返しました。Windows が **0x11C7** と呼ぶものです。

![スイッチの場所](guide/app-control-1.png)

## スイッチを切る

- **①** **設定 → プライバシーとセキュリティ → Windows セキュリティ**を開き、**Windows セキュリティを開く**をクリックします。
- **②** **アプリとブラウザー コントロール**を開き、**スマート アプリ コントロール**を選びます。
- **③** **オン**になっていたら、**スマート アプリ コントロール → 設定**を開いて**オフ**にします。

![3 つの状態](guide/app-control-2.png)

このスイッチは戻しにくいものです。以前は元に戻すのに Windows のリセットが必要でしたが、最近の更新ならアプリから直接オンに戻せます。

灰色で押せない、そもそも項目がない場合は、管理者か別のポリシーが管理しています。**イベント ビューアー → アプリケーションとサービス ログ → Microsoft → Windows → CodeIntegrity → Operational** でイベント **3077** を見てください。

![3 つの道](guide/app-control-3.png)

[Microsoft のスマート アプリ コントロール FAQ](https://support.microsoft.com/ja-jp/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)

---

解決しないときは、**0x11C7**、スイッチの状態、イベント ID を開発者に送ってください。
