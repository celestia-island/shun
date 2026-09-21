# 設計ノート：インストール時スクリプティング

状態：**ランナーは決定 —— duckscript。Python は調査済みで、組み込み可能
と実証；採用は保留。** duckscript が唯一のスクリプトランナーです；
JavaScript の選択肢は落とされました（duckscript を囲む cargo-make の
ツールセットは完全であり、そうでないところは Python の呼び出しが脱出口
です）。本ノートは、その決定と Python 組み込みの調査を記録します。

## duckscript（ランナー）

[`duckscriptsdk`](https://crates.io/crates/duckscriptsdk)（Apache-2.0、
cargo-make のスクリプト言語）は、通常の依存関係として組み込めます：コマ
ンドセットを `Context` に読み込み、shun の組み込みをカスタムコマンドと
して登録すれば、スクリプトはフロー制御と std の fs/env/net 付きで実行
できます。実現可能性の証明は `tests/scripting_duckscript.rs` にあります。
justfile 自身は組み込めません（`just` crate は CLI であり、安定したライ
ブラリー API はありません）—— duckscript が、この家族の中で組み込み可能
な一員です。

```toml
[package.metadata.shun.script]
runner = "duckscript"

[[package.metadata.shun.script.hooks]]
phase = "prepare"                 # prepare | post-install | pre-uninstall
script = "installer/prepare.dk"   # `shun build` がパック
```

shun の組み込み表面は duckscript コマンドとして登録されます：
`shun_progress`、`shun_emit`、`shun_fetch`（検証付きのダウンロード）、
さらに SDK 自身の std コマンド（fs、env、http、process、semver、…）。

スクリプトフックが走るとき、配信フローはウィザードの言語を
`SHUN_LANGUAGE` としてすべてのスクリプトステップに輸出します
（ウィザードの最初のステップで選ばれたロケール。ディスク上の
インストールマニフェストにも記録されます）。言語が未選択だった
場合、変数自体が存在しません。事実を輸出することが shun のすべて
であり、インストールされたアプリケーション自身の設定に言語を
書き込むのは payload スクリプトの仕事です。

shun ラッパーで正規化すべき落とし穴：duckscript の引数では、Windows の
バックスラッシュパスがエスケープ文字になります（正スラッシュのパスを
渡すこと）、そして代入は出力キャプチャの構文です（`x = cmd args`）。

## 組み込み Python —— 実測（2026-09 のプローブ）

以下はすべて、実際に 2 回走らせたものです：ホストの CPython 3.13.5 で
1 回、**携帯した embeddable ランタイム**で 1 回
（`python-3.13.5-embed-amd64.zip` を展開し、PyO3 の `pyembed_runner`
サンプルをその隣に置き、`python313.dll` と標準ライブラリーが携帯フォル
ダーから読み込まれるようにしたもの —— `sys.prefix` が携帯ディレクトリを
指すことを確認済み）：

| 能力 | 結果 |
| --- | --- |
| 実際の HTTPS（urllib + TLS） | OK（ローカルでは pypi.org への直接接続がネットワークで遮断；example.com とテンセントのミラーは正常） |
| ストリーミングの SHA-256 + HMAC | OK |
| AES-CTR の往復、RSA-2048 の署名/検証 | OK —— 携帯ランタイムに事前インストールした `cryptography` wheel 経由（`pip --target runtime/Lib/site-packages` + `python313._pth` で `import site` を有効化） |
| マシンの識別 | MachineGuid（winreg）、MAC（`uuid.getnode`）、C: のボリュームシリアル（ctypes `GetVolumeInformationW`）—— すべて OK |
| TPM | `tbs.dll` への ctypes の経路は正しく到達しました；プローブ機のファームウェアは TPM が無効のため、`Tbsi_Context_Create` は `TBS_E_TPM_NOT_FOUND` を返します（0x8028400F —— 注意：0x80284002 ではありません。そちらは NULL のパラメータ構造体による `TBS_E_BAD_PARAMETER` です）。呼び出し経路は検証済みです；TPM が有効なハードウェアでは、同じコードが `TPM_PT_MANUFACTURER` を読めます |

実測のサイズ：embeddable zip **10.9 MB** / 展開 **20.4 MB** /
+cryptography wheel **32.4 MB**。PyO3 ランナーのバイナリー自体は約
0.2 MB です。ネイティブの `.pyd` を含むサードパーティ wheel
（cryptography など）は変更なしで動きます —— 携帯ランタイムの中に
入れて出荷します。

本番統合に向けて記録した落とし穴：組み込みインタープリターは drop 時に
終了処理をしません —— スクリプトの実行後に stdio を明示的にフラッシュ
してください（ランナーのサンプルを参照）；`eval` は式のみを受け付けま
す；携帯ランタイムに対する pip は `--target` に加えて `._pth` の調整が
必要です（または pip を同梱する python-build-standalone ランタイム）。

## WebView2 fixed-version の組み込み —— 実測

問い：インストーラーは WebView2 エンジン自体を携帯し、自身の UI と配布
するアプリの両方を駆動できるか？ **機構的には可能 —— エンドツーエンド
で実証済み**；コストは payload です。

- v151.0.4129.101 x64 の fixed-version cab：圧縮 **307,241,094 バイト ≈
  293 MB**、展開 **661.1 MB**。
- デモシェルを、展開済みの携帯ランタイムに対して実行しました
  （`WEBVIEW2_BROWSER_EXECUTABLE_FOLDER`、これはもともと
  `webview2_available` の最初のプローブです）：UI は描画され（オフライン
  のスクリーンショットで検証）、**6 つのレンダラープロセスすべてが携帯
  フォルダーから**来ており、システムの Evergreen インストールからでは
  ありません。
- 結論：実現可能；約 300 MB のコストは**受け入れ済み**です（他のパッ
  ケージャーと同水準）。二重コピーの懸念は設計で解消されています：アー
  ティファクトが埋め込むのは 1 部だけです —— シェルは payload のランタ
  イムサブツリーから自身をブートストラップするため（`extract_prefix`
  のステージング + 展開でのハッシュを考慮した再利用）、インストーラーと
  インストール済みアプリがそれを共有します；configuration.md の
  WebView2 戦略の節を参照してください。何も無いマシンに対しては、egui
  フォールバックがゼロコストの最下限のままです。


## 組み込み Python（調査済み、実現可能）

**結論：可能です —— Rust は小さな CPython をきれいに組み込めます。**
証明は、オプトインの `python-probe` feature の背後にある
`tests/scripting_python.rs` です：[PyO3](https://pyo3.rs) の
`auto-initialize` がインタープリターをプロセスに組み込み、標準ライブラ
リー付きで本物の Python を評価し、カスタムの Rust 関数を呼び、Python の
例外を Rust のエラーに対応づけます。この feature が既定のビルドに入る
ことは決してありません；CI では、Windows の脚（`--all-features` を事前
インストール済みの CPython に対して解決する）だけが実行します。

自己完結インストーラーの携帯オプション、WebView2 戦略の表に倣って：

| オプション | 携帯物 | 備考 |
| --- | --- | --- |
| `system`（既定） | 何もなし | duckscript の `process` コマンドがインストール済みの python を呼び出し得ます；無い場合は緩やかに縮退 |
| `embeddable` | Windows embeddable パッケージ（約 12–16 MB） | 公式の `python-3.x.x-embed-amd64.zip`：`python3xx.dll` + 標準ライブラリー zip + `._pth`、管理者権限不要、レジストリ不使用 —— fixed-version WebView2 とまったく同じ哲学のプライベートランタイム |
| `standalone` | python-build-standalone（約 30–60 MB） | [Astral が管理する](https://astral.sh/blog/python-build-standalone)ディストリビューション（`uv` が同梱するもの）；クロスプラットフォーム、バージョン固定、フル機能；pip やネイティブ依存が必要なのでなければ過剰です |

スケッチ：

```toml
[package.metadata.shun.script.python]    # 任意の重量級脱出口
type = "embeddable"                      # system | embeddable | standalone
```

却下/保留した代替案：

- **RustPython**（MIT、純 Rust）—— 本番未対応を自認しており、標準ライ
  ブラリーに穴があり、C 拡張モジュールが使えません；いつかは魅力的で
  すが、今日のインストーラーには向きません。
- **PyOxidizer / `pyembed`** —— より高水準の組み込みラッパーですが、
  プロジェクトはメンテナンスモードです；ここでは PyO3 単体で十分です。

組み込み前に決めるべき問い：

- サイズの予算：オプトインする製品にとって、アーティファクトが
  +12–16 MB 増えるのは受け入れ可能か？（組み込みはマニフェストごとの
  オプトインなので、既定のアーティファクトは小さいままです。）
- バージョンの結合：PyO3 はビルドホストの CPython にリンクします；出荷
  するランタイムは一致しなければなりません。出荷するまさにそのディスト
  リビューションに対してビルドして固定します（`PYO3_PYTHON` → 展開済み
  の embeddable/standalone ディレクトリ）。
- 分離：組み込みインタープリターをプロセス内に置くか（シングルファイル
  インストーラーの UX）サブプロセスにするか（より単純なクラッシュ分離）
  —— あるいは両方を、フックごとに選ぶ。
- どのフックが Python への昇格を許されるのか（prepare のみか、それとも
  インストール後の修復パスも？）。
