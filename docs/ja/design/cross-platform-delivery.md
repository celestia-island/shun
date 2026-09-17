# 設計ノート：クロスプラットフォーム配布

状態：**Windows の表面は検証済みで、そのギャップも閉じました（2026-09）。
Linux と macOS のランタイム登録バックエンドは実装済み；モバイル
（Android/iOS）と HarmonyOS は調査済み —— 当面はスコープ外。** 本ノートが
記録するのは、(a) 自動パッケージング登録面への集中テストが出荷済みの
Windows バックエンドについて証明したこと、(b) かつて欠けていた
デスクトップショートカット / タスクバー識別 / 右クリックメニューの面
—— 現在は実装済みであり、それらを形作った実機セキュリティポリシーの
発見も併記、(c) Linux と macOS のバックエンド、そして (d) Android・iOS・
HarmonyOS の実現可能性の結論（見送り）です。

(a) の実行可能な一覧は `tests/registration_shortcuts.rs` と
`tests/msix_pack.rs` にあります。

## 1. Windows 集中テストが証明したこと（2026-09、実機）

Windows 11 の実機で本物の `InstallFlow` を実行し、結果を Windows 自身に
読み戻す（WScript.Shell COM リゾルバー、レジストリ、Windows SDK の
MakeAppx）ことで検証しました —— 自分たちの出力を点検するのではなく：

| 表面 | 結果 |
| --- | --- |
| スタートメニューの `.lnk` がシェル経由で解決される | **OK** —— `TargetPath` と `WorkingDirectory` がインストールコンテキストの宣言どおりに正確に返る；`mslnk` が手組みした合成 PIDL も正しく解決される |
| 非 ASCII のインストールパス（中文目录） | **OK** —— `顺测试目录` というインストール先が COM リゾルバーを通じてバイト完全で往復する |
| `.lnk` バイナリと MS-SHLLINK の突き合わせ | **OK** —— ヘッダー、CLSID `{00021401-…}`、フラグ群（target ID list + relative path + working dir + unicode）、ホットキーなし |
| ARP エントリー（HKCU） | **OK** —— NSIS と完全等価なフィールド一式：DisplayName/Version/Publisher/InstallLocation/DisplayIcon に加え、`UninstallString`/`ModifyPath`/`RepairString`（いずれも引用符付きで、それぞれが `/uninstall` 経由でアンインストーラー UI を起動）と DWORD の `EstimatedSize`；EstimatedSize は payload マニフェストの総量と一致 |
| アンインストール時のクリーンアップ | **OK** —— ARP キー、ショートカット、payload、アンインストーラー、ディレクトリがすべて除去される（`tests/install_local.rs` でカバー、ここで再確認） |
| MSIX マニフェスト生成 | **OK** —— アイデンティティ、4 部構成のゼロ埋めバージョン、XML エスケープ済み文字列、正スラッシュのエントリーポイント、runFullTrust |
| MSIX の実パッケージ化（MakeAppx 10.0.26100） | **OK** —— `[Content_Types].xml` + `AppxManifest.xml` を含む正当な OPC zip；生成された `dist/shundemo-0.1.0-x64.msix` は**署名ブロックを持たない**（設計どおり：Store 配布なら署名され、セルフ署名証明書を使う場合は信頼させる必要がある） |

## 2. Windows のギャップ埋め（2026-09 実装）

以下のすべては検証パスの後に実装され、各表面は実機で登録テストスイートに
よって駆動されています。

### デスクトップショートカット —— `install.desktop-shortcut`

ポリシー（`always` | `never` | `ask`、NSIS のチェックボックス慣行 —— egui
ウィザードは `ask` に対して既定でオンのトグルを表示し、ヘッドレス実行は
オンと回答します）から解決され、スタートメニューのショートカットの隣に
書き込まれます。デスクトップの解決には
**`SHGetKnownFolderPath(FOLDERID_Desktop)`** を使います ——
`%USERPROFILE%\Desktop` は決して使いません。デスクトップがリダイレクト
されている（OneDrive、ドメインポリシー）場合、後者は誤りになるからです。
アンインストール時には無条件で除去します（インストールとアンインストール
の間に設定が変わり得るためです）。

**実機での計測**：セキュリティポリシー（AV/EDR の偽ショートカット対策と
ランサムウェア保護）は、`.lnk` の作成を*とりわけデスクトップ上で*拒否する
のが通例です —— 検証機では、昇格済みシェルからの
`echo x > Desktop\probe.lnk` すら拒否され、`.tmp` ファイルは自由に書き
込めました。したがってデスクトップショートカットは**ベストエフォート**
です：拒否された書き込みは警告に格下げされ、インストールを失敗させること
は決してありません（スタートメニューショートカットと ARP エントリーが
重要な表面です）。テストスイートはマシンのポリシーをプローブし、正常経路
と緩やかな縮退経路の両方をアサートします。

### タスクバー識別 —— `install.aumid`

プログラムによるタスクバーのピン留めは**引き続きプラットフォーム設計に
よって封鎖**されています（サポート対象の API はなく、ピン留めハックは
Windows 10 で削除済みです）。出荷されたのは識別の半分です：すべての
ショートカットに、Shell COM プロパティストア経由で（`IShellLink` →
`IPersistFile` → `IPropertyStore`、`src/targets/aumid.rs` 参照 ——
`mslnk` はバイトを書くだけ）**`System.AppUserModel.ID`** を刻印するため、
タスクバーのグループ化、ジャンプリスト、そして*ユーザーが行う*ピン留めが
正しく振る舞います。既定の AUMID は `{publisher}.{product}` として生成
され、`install.aumid` がそれを上書きします。アプリは同じ値を
`SetCurrentProcessExplicitAppUserModelID` に渡すべきです。MSIX インストール
はパッケージ経由で識別を無償で得ます。刻印も同じポリシー上の理由でベスト
エフォートです（検証機は `.lnk` ファイルへの `IPropertyStore::SetValue` を
拒否します —— 0x80030005 —— ため、そこでは刻印が警告に格下げされます）。

### 右クリック動詞 —— `[[install.verbs]]`

Tier 1 を出荷しました：
`HKCU\Software\Classes\Applications\<exe>\shell\<verb>\command` 配下の
ユーザー単位 Explorer 動詞 —— 文書化された Application Registration の
表面で、昇格不要で、アプリの exe とそのショートカットに表示されます。
3 種の動詞ターゲットは、それらを実装するすべてのプラットフォームでコマンド
ラインに対応します：`data-folder`（インストールディレクトリを開く）、
`uninstall`（コピー済みのアンインストーラーを実行）、`app`（エントリー
ポイント + 引数）。アンインストールは自分が作成した動詞キーを削除し、
続いて `shell`/`Applications` コンテナは空のときに限って削除します（他の
誰かが登録した動詞は生き残ります）。Tier 2（ファイルタイプの関連付け）と
Tier 3（MSIX `FileExplorerExtension`）は今後の作業のままです。

### ディープリンク —— `install.deep-links`

配布モデルが初稿から約束していたものが、今はすべてのバックエンドで、
ユーザー単位で現実になっています：Windows は各スキームを
`HKCU\Software\Classes\<scheme>` のプロトコルクラスとして登録し（空の
`URL Protocol` マーカー + URL を `%1` として受け取るオープンコマンド）、
Linux はランチャーに `MimeType=x-scheme-handler/<scheme>;` を宣言して
`xdg-mime` で既定を主張し、macOS では合成された `Info.plist` が
`CFBundleURLTypes` を運びます。スキームは小文字の `[a-z0-9+.-]` に正規化
されます（`"MyApp://"` → `myapp`）。アンインストールは Windows のプロトコル
クラスを削除します；Linux のランチャーを削除すればハンドラーは孤立します
（`mimeapps.list` の行が不活性になる —— 記録済みであり、受容済みです）。

### なぜインストーラーは UAC 昇格を要求しないのか

この問いはデスクトップショートカットの発見の後に浮上しました。答えには
3 本の柱があります：

1. **shun が書くものはすべてユーザー単位の表面** —— HKCU、ユーザーの
   スタートメニューとデスクトップ、`%LOCALAPPDATA%`。どれひとつとして
   昇格トークンを必要としないため、UAC プロンプトは何も得させず、最悪の
   種類のプロンプトノイズ（ユーザーを眺めて承認する癖に訓練する）を加える
   だけです。これは wowsp の NSIS テンプレートと同じ取引であり、VS Code /
   Chrome の*ユーザー*セットアップの背後にあるものと同じです。
2. **昇格はそもそも `.lnk` のブロックを直しません**：このブロックは、
   デスクトップフォルダーと `.lnk` 拡張子に鍵づけされたセキュリティ製品の
   ファイルシステムフィルターです —— フィルターは ACL ではなくポリシーに
   よってプロセスを傍受し、昇格済みプロセスもフィルターされます。
   （Windows の制御付きフォルダーも同じ挙動です：アプリが許可リストに
   入っていない限り、管理者もブロックします。）正しい応答は出荷された
   ものです：警告に格下げし、スタートメニューショートカットと ARP を
   無傷のまま保ちます。
3. ***ユーザー*表面への昇格済みの書き込みは正しさの罠**：昇格済みプロセス
   はプロファイルを異なる形で解決します（管理者アカウントのデスクトップ、
   `%APPDATA%`、レジストリハイブがすべて、インストールしたユーザーのものと
   異なり得る）—— NSIS の全ユーザーショートカットの古典的バグです。昇格が
   本当に必要な場面では、そのステップが**ステップ自身**を昇格させます：
   WebView2 Evergreen ブートストラッパーは独自の `requireAdministrator`
   マニフェストを携帯するため、シェルは `asInvoker` のまま委譲します。

マシン全体のスコープ（`Program Files`、HKLM の ARP、全ユーザー
ショートカット）は、一部の製品が本当に必要とする正当な*モード*です ——
それはまさにそれとして出荷されました：意図的なオプトイン
（`install.scope`）であって、決して既定ではありません。第 6 節を参照
してください。

### 堅牢性の発見 —— いずれも修正済み

- ファイル名として不正な文字（`/\:*?"<>|`、末尾のドットやスペース）を
  含む製品名は、すべてのファイルシステムとレジストリの表面（`.lnk` 名、
  ARP キーのパス）で**ステムがサニタイズ**されます —— 製品名の中の `\` が
  レジストリサブキーを入れ子にすることはもうありません。
- ARP の `UninstallString` は `/uninstall` を渡しますが、インストーラー
  シェルのヘッドレスパーサーは `--uninstall` しか受け付けていません
  でした —— Windows の設定で「アンインストール」をクリックすると、
  アンインストールではなくウィザードが起動していました。両方の綴りが
  解析されるようになりました。

## 3. Linux と macOS（ランタイムバックエンドは実装済み；パッケージング成果物は次）

**両方とも手に負え、そして両方とも Windows と一つの厳しい限界を共有
します：プログラムによるタスクバー/Dock のピン留めは、どこにも存在し
ません。** ランタイムの `Registration` バックエンドは出荷済みです；ビルド
側のパッケージング成果物（`tauri-bundler` 経由の deb/rpm、DMG）は、ネイ
ティブなビルドホストが必要なため、引き続き今後の作業です。

### Linux —— `LinuxRegistration`（src/targets/freedesktop.rs）

Windows バックエンドが行うことはすべて freedesktop の慣行に写像されます。
すべてユーザー単位（`~/.local/share/...`）で、昇格は不要です：

- **ランチャー登録** = `<product>.desktop`（Name、Exec、Icon は
  `install.icon` から、Categories、そして決定的な **`StartupWMClass`** =
  エントリー実行ファイルのステム —— ユーザーが行うタスクバー/Dock のピン
  留めが正しいアイコンの下にグループ化されるようになるフィールド）を
  `~/.local/share/applications` に書き込み、続けてそれに対して
  `update-desktop-database` を実行します（ツールが無い場合はスキップして
  も安全です —— デスクトップ環境は遅延的に再スキャンします）；
- **右クリック動詞 + アンインストール項目** = `Actions=` +
  `[Desktop Action <id>]` グループ —— 常に **Uninstall** アクションを含み
  ます。なぜなら GNOME Software / KDE Discover は自前のパッケージバック
  エンドが追跡するアプリしか一覧にしないからです：shun でインストール
  されたアプリはそこに決して現れません。3 種の動詞ターゲットは
  `xdg-open`、アンインストーラー、エントリーポイント + 引数に対応します；
- **実行ビットの復元** —— payload アーカイブは全エントリーを 0644 で
  運ぶため、バックエンドはエントリーポイントとコピーしたアンインストー
  ラーを 0755 に chmod し戻します；
- **登録解除** = `.desktop` の削除 + データベースの更新。

`.desktop` ライターは純粋なデータ処理であり、すべてのプラットフォームで
コンパイルされ（ユニットテストもされます）；プロセスを起動する半分だけが
Linux でゲートされています。タスクバー/Dock の**ピン留めは不可能のまま**
です（クロスデスクトップの API はありません：GNOME のお気に入りは内部の
gsettings キーであり、KDE のピンは文書化されていない appletsrc に存在
します；ユーザー操作として扱ってください）。ファイルマネージャーのコン
テキストメニュー（Nautilus スクリプト / Dolphin サービスメニュー）はス
コープ外のままです。

**パッケージング形式**（引き続きビルド側、今後の作業）：**tarball/ポータ
ブル（shun は既に持っています）+ deb（[cargo-deb]）+ rpm
（[cargo-generate-rpm]）** が最良の部分集合です —— まさに `tauri-bundler`
が出力するものです（非 Tauri の payload にも使えるライブラリーであり、
[cargo-packager] も同様です）。AppImage = 中；Flatpak = 中〜高；snap =
高、見送り。正直な「あらゆるディストロで動く」の限界：glibc は前方互換の
みであり、**musl のスタティックビルドは WebView アプリを運べません**
（webkit2gtk が GTK の C スタック全体を引きずるため）—— 配布シェルは
musl スタティックにできますが、配布される Tauri アプリは、サポート対象の
最も古い webkit2gtk-4.1 ベースラインに対してビルドしなければなりません
（Ubuntu 22.04 / Debian 12 / Fedora 37+ の世代）。

### macOS —— `MacOSRegistration`（src/targets/macos.rs）

- **登録** = エントリー実行ファイルが属する `.app` バンドルを特定する
  （最も近い `.app` の先祖）；payload が何も運んでこなければ最小の
  `Info.plist` を合成します（`plist.rs`、純粋で、あらゆるプラットフォーム
  でユニットテスト済み）；エントリーポイントの実行ビットを復元します；
  インストール全体から継承した `com.apple.quarantine` を再帰的に**剥離**
  します（ブラウザーはインストーラーに検疫印を押し、macOS のコピーは
  xattr を保存します —— これをしないと、配布されたアプリは、ユーザーが
  既に答えたはずの Gatekeeper の関門を継承します）；それからバンドルを
  `lsregister -f` に掛けます —— Spotlight と Launchpad が続きます。登録
  解除 = `lsregister -u`（ファイルは汎用のアンインストールパスで削除
  されます）。素の実行ファイル payload（`.app` なし）は no-op として登録
  されます —— ポータブルの慣行です；
- **Dock のピン留め**：**サポート対象の API はありません**
  （`defaults write com.apple.dock` + `killall Dock` のハックはユーザー
  設定を踏み荒らし、最近の macOS では信頼できません）—— Launchpad/
  Spotlight への露出（LS 登録）が、発見可能性の面での等価物です；
- **署名は、本物の配布にとって必須のプロセス作業**のままです：Developer
  ID + hardened runtime + `notarytool` + staple；さらに、ダウンロード
  されたシェルは**トランスロケーションされます** —— 自己パスに関する
  仮定はそれに合わせて扱ってください；
- **パッケージング成果物**（今後の作業）：DMG は tauri-bundler /
  `hdiutil` 経由、`.pkg` は管理者フロー専用、Homebrew cask が一つの
  チャンネルです。**universal2** で出荷します（デュアルビルド + `lipo` +
  再署名）。

[cargo-deb]: https://github.com/kornelski/cargo-deb
[cargo-generate-rpm]: https://crates.io/crates/cargo-generate-rpm
[cargo-packager]: https://github.com/crabnebula-dev/cargo-packager

## 4. Android と iOS（調査済み）

**まず枠組みから：モバイルでは「インストール」はプラットフォーム所有で
署名検証される行為です。アプリディレクトリの zstd tar をユーザーが選んだ
場所へストリーミングするような等価物は存在しません。** shun のモバイルに
おける正直な役割は、**ビルド/パッケージング/署名のパイプライン**
（「モバイル向け cargo-dist」）であって、インストーラーランタイムでは
ありません。

### Android

- **パッケージングの機構**：APK = 署名付き zip（DEX + リソース + ABI ごと
  の `.so`）；パイプラインは `aapt2` → `d8`/`r8` → パッケージ →
  `zipalign` → `apksigner`。AAB は Play の公開形式です（新規アプリには
  必須）；サイドロードには具体的な APK が必要です
  （`bundletool build-apks` + 再署名）。Rust ターゲットはホストツール付き
  の Tier 2 です。
- **生きているツール類（2025–2026）**：Tauri 2 の `tauri android build`
  （cargo-mobile2 + Gradle のラップ；keystore は `keystore.properties` 経
  由）、**cargo-ndk**（メンテナンス中）、**`apk` crate**（Gradle 不要の
  aapt2/d8/zipalign/apksigner）。**xbuild と cargo-apk は死んでいるか休眠
  中** —— それらの上に築かないでください。Tauri 2 モバイルは公式に安定
  しており、Android 側は iOS 側より成熟しています。
- **ランタイムインストーラー：実現不可能/無意味。** OS のパッケージマ
  ネージャーは署名付き APK から、ユーザー向けの確認付きでインストールし
  ます（サイレントなのは device-owner/MDM のみ）；**APK それ自体がインス
  トーラーです**。Play ポリシーはさらに**自己更新/実行コードのダウンロー
  ドを禁止**しています —— shun 流のアップデーターランタイムは、Play で
  配布されるどんなアプリにとってもポリシー違反になります。サイドロード
  は設計によって劣化します：**未検証開発者サイドロードの強制が
  2026-09-30 に始まります**（未検証アプリは、24 時間の待ちを伴う多段階
  フローに掛かります）。
- **ショートカット**：ホーム画面のアイコンとアプリショートカット
  （`shortcuts.xml`、`ShortcutManager`、`requestPinShortcut`）は**アプリ
  宣言のみ**です —— インストーラー時点での注入 API は存在しません。ビル
  ド時の等価物は、shun がパックする APK に `shortcuts.xml`/intent-filter
  を生成することです。

### iOS

- **パッケージングの機構**：IPA = `Payload/App.app` +
  `embedded.mobileprovision`（App ID + エンタイトルメント + 証明書 + Ad
  Hoc の UDID 許可リスト）を含む zip。チャンネル：App Store、TestFlight、
  Ad Hoc（100 デバイス/種類/年）、Enterprise。**署名ツールチェーン
  （codesign、xcodebuild、キーチェーン）は macOS 専用** —— ハードなホスト
  要件です。
- **ランタイムインストーラー：実現不可能**、ただし 2 つの隙間を除き
  ます：(a) Ad Hoc の OTA マニフェスト
  （`itms-services://?...manifest.plist`）—— 簡単、合法、ニッチ；(b) EU
  の Web Distribution / 代替マーケットプレイス制度 —— 実在しますが、
  Apple の適格性 + 公証 + 2026 年 10 月の料金条件（5% の Core Technology
  Commission）で門が守られており、EU 限定です。無料 Apple ID のサイド
  ロード（AltStore/Sideloadly）は 7 日/3 アプリのホビイスト経路であって、
  製品化できるチャンネルではありません。
- **ショートカット**：どのチャンネルでも、インストーラー時点に存在する
  ものは何もありません —— ホーム画面のアイコン、URL スキーム、Universal
  Link はすべてアプリ宣言であり、署名検証されます。
- **egui フォールバック**：Android = `android-activity` + winit + wgpu
  （Vulkan/GLES）；iOS = winit + wgpu（Metal）を FFI 経由で UIKit ホスト
  に組み込み + Xcode プロジェクト。どちらにもターンキーの話はありませ
  ん —— shun のパッケージングパイプラインこそが、まさに欠けている部品
  です。

## 5. HarmonyOS（調査済み）

**結論を先に：2026 年の「shun は HarmonyOS をサポートする」が正直に意味
できるのは、ただ一つのことです —— リリース署名済みの HAP/APP 成果物を生
成するビルド時のパッケージング/署名ターゲットと、`hdc install` による開
発機フロー。ランタイムインストーラー（shun の NSIS 側の半分）は、
HarmonyOS NEXT には合法的にも技術的にも基盤がありません。**

情勢（2025–2026）：HarmonyOS NEXT（5.0、2024 年 10 月）は APK 互換層を
落としました；狙うべき線は HarmonyOS 6+（API 20/23）で、中国限定、
AppGallery 限定、中国 OS 市場の約 19% です。OpenHarmony がオープンな基盤
であり、商用 HarmonyOS はその上に載る Huawei の製品です —— パッケージャー
が狙うのは商用の方です。

- **パッケージ形式**：HAP（zip：`module.json5`、ArkTS バイトコード、
  ネイティブの `libs/<abi>/*.so`）；HSP/HAR の共有パッケージ；`.app` =
  AppGallery への提出パック（`pack.info`）。ツール類は CLI で使えます：
  `ohpm` + `hvigorw assembleHap` + `hap-sign-tool` + `app_packing_tool.jar`
  （ヘッドレスな CI ビルドを公式にサポート）。
- **署名**：SHA256withECDSA；`.p12` キーストア + `.cer` + `.p7b` プロ
  ファイル（バンドル名、権限、デバッグではデバイス UDID の許可リスト）；
  証明書は **Huawei が AppGallery Connect 経由で発行**します（個人登録は
  無料 —— Apple 式の料金はありません）。
- **Rust**：`aarch64/armv7/x86_64-unknown-linux-ohos` は**ホストツール
  付きの Tier 2** です（1.78 から rustup 対応）。コミュニティの `ohos.rs`
  ツールチェーン（`cargo-ohos`、`napi-ohos`、`ohos-openssl`）が接着剤
  です；Rust コア + ArkTS シェルは実証済みのアーキテクチャです
  （RustDesk OHOS）。**egui はブロック**されています：winit には上流の
  OHOS バックエンドがありません（コミュニティのベータのみ）。**Tauri**：
  公式だが未マージの `feat/open-harmony` ブランチ（wry/tao パッチ、
  `cargo tauri ohos` CLI）は今日動きますが、動きが速い —— 数か月ごとの
  再ピンを想定してください。
- **配布に関して正直に**：消費者向けのサイドロードは事実上閉じています
  （AppGallery のみ；`hdc install` には開発者モード + Huawei 署名 + UDID
  が必要です）。指定デバイスのリリース：年 100 デバイス、90 日の有効
  期間。エンタープライズ配布は現在、Qingyun エンタープライズ PC に限定
  されます。**HarmonyOS PC** は実在します（ARM、ストア配布、サイドロード
  はまだ不可）—— Huawei は PC のサイドロードを後日開く*意思を表明*して
  います；これが、いつかデスクトップ配布ランタイムをあそこで正当化し得る
  唯一のウォッチ項目です。

作業量一覧：HAP パッケージングターゲット**中**；Rust のクロスステップ
**易〜中**（リンカーには SDK の clang ラッパー、TLS には ohos-openssl）；
Tauri-on-OHOS のパッケージング**中〜難**（上流未マージ）；egui フォール
バック**難**（winit なし）；ランタイムインストーラー/フラッシャー**実現
不可能**。

## 6. インストールスコープと宣言的ウィザード（2026-09 実装）

### インストールスコープ —— `install.scope = user | machine | ask`

ユーザー単位は既定のままです（「なぜインストーラーは UAC 昇格を要求しない
のか」を参照）。`machine` —— あるいは `ask` への「すべてのユーザー」回答 ——
は、すべての登録表面をそのマシン全体での等価物へ反転させます：ARP
エントリーは **HKLM** 配下へ、ショートカットは**全ユーザーのスタート
メニュー**（`%ProgramData%`）へ、デスクトップショートカットは**パブリック
デスクトップ**（`FOLDERID_PublicDesktop`）へ、動詞とディープリンクは
`HKLM\Software\Classes` 配下へ。シェルはフローの実行**前に**解決結果を検出
し、未昇格であればユーザーの回答を携えて `runas` 動詞で自身を再起動しま
す（`--silent --mode=… --dir=… --scope=machine`）：UAC 同意はただ一つのプロ
ンプトであり、それを必要とするモードにのみ表示されます —— まさにブート
ストラッパーパターン、約束どおりです。アンインストールも対称です（ARP の
`UninstallString` がアンインストーラーを起動し、それが同じ方法で昇格しま
す）。マシンスコープは Windows 専用で、Linux/macOS のバックエンドは明示
的に拒否します。統合テストは、runner が昇格済みでなければスキップします
（管理者シェルからの `cargo` がそれを実機で実行します）。

### ウィザードパイプライン —— `[[package.metadata.shun.steps]]`

ウィザードは、固定の モード → インストール 連鎖ではなく、**宣言的で、
順序付きで、自由に合成できる**パイプラインになりました。5 種類のステップ
です：

```toml
[[package.metadata.shun.steps]]
kind = "mode"                    # 配布モード + ディレクトリ；`ask` の
                                 # トグル（デスクトップショートカット、スコープ）を内蔵
[[package.metadata.shun.steps]]
kind = "scope"                   # スタンドアロンの user/machine 選択
[[package.metadata.shun.steps]]
kind = "license"                 # ライセンスペイン（license / license-locales）
[[package.metadata.shun.steps]]
kind = "content"                 # カスタム markdown ペイン
title = "Release notes"
markdown = "notes.md"         # マニフェストからの相対
[[package.metadata.shun.steps]]
kind = "install"                 # 配布実行（ちょうど 1 つ必須）
```

`steps` を宣言しない場合は既定のパイプライン（モード → 宣言されていれば
ライセンス → インストール）になり、従来の `custom-steps` は各自の `after`
キーの後ろに注入されます；両方を宣言するのは設定エラーであり、`install`
ステップが 0 個または複数個でも同じです。スコープの質問とデスクトップ
ショートカットのトグルは、`ask` ポリシーがペインに出会う場所ならどこに
でも現れます：モードステップに内蔵するか、スタンドアロン（`scope`）に
するか —— 開発者が合成します。コンテンツとライセンスのドキュメントは
**ビルド時に**読み込まれて `shun-steps.json` にインライン化されます
（`ShunConfig::resolve_steps`）ので、ランタイムインストーラーはファイル
依存を何も運びません。egui フォールバックはパイプライン全体を描画します
（ステップごとのレール、ライセンスのゲート、戻る/次へのナビゲーション）；
Tauri の `ShellView` は、解決済みのステップを web フロントエンドに公開
します。

## 7. ポートフォリオの状態（2026-09）

| 能力 | Win | Linux | macOS | Android | iOS | HarmonyOS |
| --- | --- | --- | --- | --- | --- | --- |
| ランタイムインストール + 登録 | ✅ ユーザー + マシンのスコープ | ✅ `.desktop` バックエンド（ユーザー） | ✅ `.app` バックエンド（ユーザー） | 見送り | 見送り | 見送り |
| デスクトップ/スタートメニューのショートカット | ✅ 両方（ポリシー駆動） | ✅ ランチャーエントリー | ✅（Launchpad/Spotlight） | 該当なし | 該当なし | 該当なし |
| タスクバー/Dock のピン留め | 識別のみ（AUMID 刻印） | 識別のみ（StartupWMClass） | 識別のみ（LS） | 該当なし | 該当なし | 該当なし |
| 右クリックメニュー | ✅ Tier 1 動詞 | ✅ Desktop Actions | NSServices は今後 | 見送り | 見送り | 見送り |
| ディープリンク | ✅ プロトコルクラス | ✅ MimeType + xdg-mime | ✅ CFBundleURLTypes | 見送り | 見送り | 見送り |
| アンインストールの方式 | ✅ ARP + 自己削除 | ✅ Action ベース | ✅ LS 登録解除 + ファイル | OS | OS | OS |
| パッケージング成果物 | ✅ インストーラー + MSIX | tarball ✅；deb/rpm は今後 | bundle ✅；DMG は今後 | 見送り | 見送り | 見送り |
| 署名の現実 | Authenticode / Store | 任意 | 必須（プロセス作業） | keystore / Play | Apple の証明書 | Huawei AGC |

**決定による見送り（2026-09）**：Android、iOS、HarmonyOS —— 第 4–5 節の
調査結果はそのまま有効ですが、どれも予定には入っていません。

**次の順番**：

1. `tauri-bundler` 経由の deb/rpm パッケージング成果物（Linux CI）、
   macOS ホストレーンでの DMG。
2. Windows の右クリックメニュー Tier 2（ファイルタイプの関連付け）——
   必要とする利用者が現れれば。
3. **決して行いません**：いかなるスマートフォン OS 上でのランタイムイン
   ストーラー/ショートカット注入；いかなる場所でのタスクバー/Dock のプロ
   グラムによるピン留め；snap（その複雑さに見合うと証明されるまでは）。
