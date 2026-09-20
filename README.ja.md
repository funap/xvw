<div align="center">

# ⚡ xvw

### **Rust製の高速でGPUアクセラレーションに対応したバイナリエディタ**

[English](README.md) | [日本語](README.ja.md)

<br />

**[GPUI](https://gpui.rs/)**（[Zed](https://zed.dev/) を支える高性能GPU UIフレームワーク）を採用。  
リバースエンジニアリング、ファームウェア解析、バイナリフォーマットのデバッグ用途に向けて、軽快かつ直感的で多機能なエディタとして設計されています。

[![GitHub License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)
[![GPUI Powered](https://img.shields.io/badge/powered%20by-GPUI-blueviolet.svg)](https://gpui.rs/)
[![Repository](https://img.shields.io/badge/github-funap%2Fxvw-informational.svg)](https://github.com/funap/xvw)

<br />

<p align="center">
  <img src="docs/images/main.png" alt="xvw メインウィンドウ" width="900" style="border-radius: 8px; box-shadow: 0 8px 30px rgba(0,0,0,0.3);" />
</p>

</div>

---

> [!WARNING]
> **アルファ版に関する注意事項および免責事項**  
> `xvw` は現在初期アルファ段階にあり、十分な検証や堅牢化が行われていません。編集を行う前には必ず**ファイルの確実なバックアップ**を取ってください。自己責任でご使用ください。  
> フィードバック、バグ報告、コントリビューションを大歓迎しています！お気軽に [Issue の作成](https://github.com/funap/xvw/issues) や Pull Request の送信をお願いします。

---

## ✨ 主な機能

- ⚡ **数GBクラスの大容量ファイルを瞬時にロード**  
  メモリマップドI/O（`memmap2`）により、ギガバイト単位のファイルやディスクダンプでも、わずかなメモリ消費とゼロ遅延で瞬時に開くことができます。
- 🧬 **インタラクティブなKaitai Struct構造解析**  
  `.ksy` フォーマット定義を実行時に読み込み、複雑なバイナリ形式（ZIP、ELF、Mach-O、PNGなど）を構造化ツリーナビゲーション、インラインカラーハイライト、解析値の表示によって分析できます。
- ✂️ **カスタム論理改行**  
  任意の場所で `Enter` を押すことで、固定された16バイト単位のグリッドに縛られず、パケットやレコードの自然な境界で改行できます。`Cmd+J` / `Ctrl+J` で行を結合できます。
- 🔍 **リアルタイムデータインスペクタ**  
  選択したバイトや範囲を、`i8`〜`i64`、`u8`〜`u64`（リトル／ビッグエンディアン）、`f32`/`f64`、Unixタイムスタンプ、2進数ビット列、文字列表現などにリアルタイムで同時デコードして確認できます。
- 📊 **2Dビジュアルマップ＆エントロピー表示**  
  バイト値をグレースケール、バイトカテゴリ、レインボーなどのカラーモードで2Dビットマップとして可視化し、コード領域、圧縮領域、暗号化ペイロードなどを視覚的に判別できます。
- ⚖️ **2画面同期バイナリDiff（差分比較）**  
  2つのバイナリファイルを左右に並べて比較可能。同期スクロール、差分カウンター、明確な差分ハイライトを備えています。
- 🌐 **40種類以上の文字コードに対応**  
  UTF-8、UTF-16、Shift-JIS、EUC-JP、GB18030、Big5、ISO-8859各種、Windowsコードページ、レガシー文字セットによる文字列のデコードと検索が可能です。
- 📋 **多彩な「形式を指定してコピー」**  
  選択範囲を C/C++ 配列、Rust 配列、JSON 配列、Base64 文字列、Hex ストリーム、印刷可能テキスト、フォーマット済み Hex ダンプとして素早くエクスポートできます。
- 🎯 **アドレスジャンプ＆範囲選択**  
  任意のアドレス・オフセット（16進数、10進数、相対指定、パーセンテージ）へのジャンプや、アドレス範囲（例: `0x20..0x1ff`）の選択が可能です。
- ⌨️ **Vimライクなキーバインド**  
  `h`/`j`/`k`/`l` による直感的なカーソル移動、`Shift` キーを組み合わせた選択範囲の拡張、`/` による16進パターン・テキスト・正規表現での検索に対応しています。

---

## 📸 機能紹介

### 🧬 Kaitai Struct によるバイナリ構造解析
フォーマット定義を動的に読み込み、生のバイト列上にマッピングされたフィールド、オフセット、ネストしたデータ構造を直接検査できます。

<p align="center">
  <img src="docs/images/structure.png" alt="Kaitai Struct 構造解析" width="850" style="border-radius: 6px;" />
</p>

### 📊 2Dビジュアルマップ / エントロピー解析
2Dヒートマップとカスタムパレットモードにより、バイトパターンの特徴、圧縮領域の境界、暗号化ブロックなどを一目で特定できます。

<p align="center">
  <img src="docs/images/visual_map.png" alt="2Dビジュアルマップ" width="850" style="border-radius: 6px;" />
</p>

### ⚖️ 2画面同期バイナリ比較（Diff）
ファームウェアのパッチ検証、ファイルの変更点確認、バイナリの差分追跡を、同期スクロール付きのデュアルペインDiffで行えます。

<p align="center">
  <img src="docs/images/diff.png" alt="バイナリDiff表示" width="850" style="border-radius: 6px;" />
</p>

### 🔍 リアルタイムデータインスペクタ＆デコーダ
エンディアンを切り替えながら、複数の数値表現やテキスト形式にその場でデコードして値を検査できます。

<p align="center">
  <img src="docs/images/inspector.png" alt="データインスペクタ" width="850" style="border-radius: 6px;" />
</p>

---

## 🚀 はじめ方

### インストール

#### ビルド済みバイナリ（推奨）

[Releases](https://github.com/funap/xvw/releases) ページからお使いのOS向けの最新リリースをダウンロードしてください:

- **macOS** (`.dmg`):
  1. `xvw-<version>-aarch64-apple-darwin.dmg` をダウンロードします。
  2. ディスクイメージを開き、**`xvw.app`** を **Applications**（アプリケーション）フォルダにドラッグ＆ドロップします。
  
  > [!TIP]
  > **macOS: 「“xvw”は壊れているため開けません」と表示される場合**  
  > macOSのGatekeeperによって「壊れている」と警告され起動がブロックされる場合があります。これはオープンソースのリリースであり、Appleの有料開発者プログラムによる公証（Notarization）を受けていないためです。バイナリ自体は安全であり破損していません。  
  > `xvw` の実行を許可するには、**ターミナル**を開いて以下を実行してください:
  > ```bash
  > xattr -cr /Applications/xvw.app
  > ```
  > *(または、Finderで `xvw.app` を **Controlキーを押しながらクリック**（右クリック）して「**開く**」を選択し、確認ダイアログで「**開く**」をクリックしてください)*

- **Linux** (`.deb` / `.rpm`):
  - **Debian / Ubuntu**: `sudo dpkg -i xvw_<version>_linux_amd64.deb`
  - **Fedora / RHEL**: `sudo rpm -i xvw-<version>-1.linux.x86_64.rpm`
- **Windows** (`.zip`):
  - `xvw-<version>-x86_64-pc-windows-msvc.zip` をダウンロード・解凍し、`xvw.exe` を実行してください。

#### ソースコードからビルド

##### 前提条件

- **Rust**（2024 edition / 最新の安定版）  
  [rustup.rs](https://rustup.rs/) からインストール:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

##### ビルドと実行

```bash
# リポジトリのクローン
git clone https://github.com/funap/xvw.git
cd xvw

# ビルドおよび実行
cargo run --release
```

### コマンドラインでの使用方法

`xvw` は強力なCLIインターフェースを備えており、目的の解析ワークフローを直接起動できます:

```bash
# 特定のバイナリファイルを開く
xvw path/to/binary_file.bin

# プロジェクトフォルダ / ワークスペースを開く
xvw --folder path/to/folder

# ファイルを開き、自動的にKaitai Structフォーマット定義を適用する
xvw firmware.bin --ksy specs/firmware.ksy

# 2つのバイナリファイルを左右に並べて比較する
xvw --diff original.bin patched.bin

# 特定のサイドバーパネルを開いた状態で起動する
# (選択肢: files, strings, structure, inspector, map, checksum, bookmarks)
xvw binary.bin --panel map
```

---

## ⌨️ 主なキーバインド

### 移動・編集

| 操作 | macOS | Linux / Windows |
|---|---|---|
| **カーソル移動** | `h` / `j` / `k` / `l` または 矢印キー | `h` / `j` / `k` / `l` または 矢印キー |
| **選択範囲の拡張** | `Shift + h/j/k/l` | `Shift + h/j/k/l` |
| **ファイルの先頭へ移動** | `Cmd + Home` / `Home` | `Ctrl + Home` / `Home` |
| **ファイルの末尾へ移動** | `Cmd + End` / `End` | `Ctrl + End` / `End` |
| **アドレス・範囲指定ジャンプ** | `Cmd + L` / `Ctrl + G` | `Ctrl + L` / `Ctrl + G` |
| **カスタム改行の挿入** | `Enter` | `Enter` |
| **行の結合** | `Cmd + J` | `Ctrl + J` |
| **カスタム改行のリセット** | `Cmd + Shift + Backspace` | `Ctrl + Shift + Backspace` |
| **挿入 / 上書きモード切り替え** | `Insert` | `Insert` |

### 検索・ワークスペース

| 操作 | macOS | Linux / Windows |
|---|---|---|
| **検索（インライン）** | `/` または `Cmd + F` | `/` または `Ctrl + F` |
| **検索パネル（全体スキャン）** | `Cmd + Shift + F` | `Ctrl + Shift + F` |
| **次 / 前の一致箇所** | `Cmd + G` / `Cmd + Shift + G` | `F3` / `Shift + F3` |
| **ファイルを開く（ダイアログ）** | `Cmd + O` | `Ctrl + O` |
| **フォルダを開く** | `Cmd + Shift + O` | `Ctrl + Shift + O` |
| **左サイドバーの表示切替** | `Cmd + B` | `Ctrl + B` |
| **エディタの分割（右 / 下）** | `Cmd + \` / `Cmd + Shift + D` | `Ctrl + \` / `Ctrl + Shift + D` |
| **タブの切り替え** | `Cmd + 1` .. `Cmd + 9` | `Ctrl + 1` .. `Ctrl + 9` |
| **Kaitai Struct（`.ksy`）の読み込み** | `Cmd + Shift + S` | `Ctrl + Shift + S` |
| **インライン構造体表示の切替** | `Cmd + Shift + V` | `Ctrl + Shift + V` |
| **Hexダンプとしてコピー** | `Cmd + Shift + C` | `Ctrl + Shift + C` |
| **設定** | `Cmd + ,` | `Ctrl + ,` |

---

## 🌐 対応文字コード

組み込みで40種類以上の文字エンコーディングに対応し、バイナリデータのデコードおよび検索が可能です:

- **Unicode & ASCII**: UTF-8, UTF-16 LE, UTF-16 BE, ASCII
- **日本語**: Shift-JIS (CP932 / Windows-31J), EUC-JP, ISO-2022-JP
- **中国語・韓国語**: GBK, GB18030, Big5, EUC-KR
- **ISO-8859 ファミリー**: ISO-8859-1 〜 ISO-8859-16（Latin 1–10、キリル文字、アラビア文字、ギリシャ文字、ヘブライ文字、ケルト文字など）
- **Windows コードページ**: Windows-1250 〜 Windows-1258
- **レガシー & Mac**: KOI8-R, KOI8-U, Mac OS Roman, IBM866

---

## 🤝 コントリビューション

コントリビューション、Issue、機能提案を歓迎しています！

- 🐛 **バグの報告や機能提案**: GitHubの [Issue](https://github.com/funap/xvw/issues) を作成してください。
- 💡 **開発への貢献**: Pull Requestは大歓迎です！大規模なアーキテクチャの変更を行う場合は、事前に既存のIssueを確認するか、Discussionを開始してください。

---

## 📄 ライセンス

本プロジェクトは [MIT License](LICENSE) のもとで公開されています。

- テーマ: [gpui-kit](https://github.com/longbridge/gpui-kit)
- アイコン: [Lucide](https://lucide.dev)

