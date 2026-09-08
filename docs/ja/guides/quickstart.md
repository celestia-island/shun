# クイックスタート

shun は两半で構成されます：**ビルド側**（payload のパック、配布マニフェストの解決）と
**ランタイム側**（フローを駆動するシェル）。

## デモを試す

```bash
cargo run --example demo_flash                        # 書き込み候補デバイスの列挙
cargo run --example demo_install                      # ShunDemo.shun 生成 + ローカルインストール
cargo run --example demo_install -- --portable        # ポータブルインストール（レジストリ不使用）
cargo run --example demo_install -- --uninstall       # アンインストール（痕跡を完全除去）
```

`demo_install` はインストーラーパッケージ `ShunDemo.shun` を生成し、ストリーミング
進捗付きで展開し、ローカルモードでは 直接 Windows 登録を行います：ユーザー単位の ARP
エントリー（設定 → アプリ）、スタートメニューのショートカット、自己コピーする
アンインストーラー。ポータブルモードは `.shun-portable` マーカーのみ書き込み、
レジストリには一切触れません。

## デモシェルを実行

```bash
pnpm --dir shell/web install
cargo run -p shun_demo_shell
```

シェルはビルド時にデモ payload を埋め込み（シングルファイルインストーラー
パターン）、`shell/Cargo.toml` → `[package.metadata.shun]` で宣言された
配布モードを描画します。
