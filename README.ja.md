<div align="center">

# ⚡ xvw

### **ZedのGPUIフレームワークで構築された、モダンで高速なクロスプラットフォーム・バイナリエディタ**

[English](README.md) | [日本語](README.ja.md)

<br />

**[GPUI](https://gpui.rs/)**（[Zed](https://zed.dev/) を支える高性能GPU UIフレームワーク）を採用。  
リバースエンジニアリング、ファームウェア解析、バイナリフォーマットのデバッグに向けた、軽快かつ直感的で多機能な設計。

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
> `xvw` は現在初期アルファ段階であり、十分な検証や堅牢化は未実施。編集前には必ず**ファイルの確実なバックアップ**を推奨（自己責任での利用）。  
> フィードバック、バグ報告、コントリビューションを歓迎。[Issue の作成](https://github.com/funap/xvw/issues) や Pull Request の送信はお気軽に。

---

## ✨ 主な機能

- ⚡ **数GBクラスの大容量ファイルを瞬時にロード**  
  メモリマップドI/O（`memmap2`）により、ギガバイト単位のファイルやディスクダンプでも極小のメモリ消費とゼロ遅延で瞬時にロード可能。
- 🧬 **インタラクティブなKaitai Struct構造解析**  
  `.ksy` フォーマット定義の動的読み込みにより、複雑なバイナリ形式（ZIP、ELF、Mach-O、PNGなど）を構造化ツリーナビゲーション、インラインカラーハイライト、解析値表示で分析可能。
- ✂️ **カスタム論理改行**  
  固定された16バイトグリッドに縛られず、任意の場所で `Enter` によるパケットやレコード境界での改行に対応（`Cmd+J` / `Ctrl+J` での行結合も可能）。
- 🔍 **リアルタイムデータインスペクタ**  
  選択したバイトや範囲を、`i8`〜`i64`、`u8`〜`u64`（リトル／ビッグエンディアン）、`f32`/`f64`、Unixタイムスタンプ、2進数ビット列、文字列表現などにリアルタイムで同時デコード表示。
- 📊 **2Dビジュアルマップ＆エントロピー表示**  
  バイト値をグレースケール、バイトカテゴリ、レインボーなどのカラーモードで2Dビットマップ可視化。コード領域、圧縮領域、暗号化ペイロードなどを視覚的に識別可能。
- ⚖️ **2画面同期バイナリDiff（差分比較）**  
  2つのバイナリファイルを左右に並べて比較。同期スクロール、差分カウンター、明確な差分ハイライト機能を搭載。
- 🌐 **40種類以上の文字コードに対応**  
  UTF-8、UTF-16、Shift-JIS、EUC-JP、GB18030、Big5、ISO-8859各種、Windowsコードページ、レガシー文字セットによる文字列デコードおよび検索に対応。
- 📋 **多彩な「形式を指定してコピー」**  
  選択範囲を C/C++ 配列、Rust 配列、JSON 配列、Base64 文字列、Hex ストリーム、印刷可能テキスト、フォーマット済み Hex ダンプとして高速エクスポート。
- 🎯 **アドレスジャンプ＆範囲選択**  
  任意のアドレス・オフセット（16進数、10進数、相対指定、パーセンテージ）へのジャンプや、アドレス範囲（例: `0x20..0x1ff`）の選択に対応。
- ⌨️ **Vimライクなキーバインド**  
  `h`/`j`/`k`/`l` による直感的なカーソル移動、`Shift` キーによる選択範囲の拡張、`/` による16進パターン・テキスト・正規表現検索に対応。

---

## 📸 機能紹介

### 🧬 Kaitai Struct によるバイナリ構造解析
フォーマット定義の動的読み込みにより、生のバイト列上にマッピングされたフィールド、オフセット、ネストしたデータ構造を直接検査可能。

<p align="center">
  <img src="docs/images/structure.png" alt="Kaitai Struct 構造解析" width="850" style="border-radius: 6px;" />
</p>

### 📊 2Dビジュアルマップ / エントロピー解析
2Dヒートマップとカスタムパレットモードにより、バイトパターンの特徴、圧縮領域の境界、暗号化ブロックなどを一目で特定可能。

<p align="center">
  <img src="docs/images/visual_map.png" alt="2Dビジュアルマップ" width="850" style="border-radius: 6px;" />
</p>

### ⚖️ 2画面同期バイナリ比較（Diff）
同期スクロール付きデュアルペインDiffによる、ファームウェアのパッチ検証、ファイルの変更点確認、バイナリ差分追跡。

<p align="center">
  <img src="docs/images/diff.png" alt="バイナリDiff表示" width="850" style="border-radius: 6px;" />
</p>

### 🔍 リアルタイムデータインスペクタ＆デコーダ
エンディアン切り替えに対応し、複数の数値表現やテキスト形式にその場でデコードして値を検査可能。

<p align="center">
  <img src="docs/images/inspector.png" alt="データインスペクタ" width="850" style="border-radius: 6px;" />
</p>

---

## 🚀 はじめ方

### インストール

#### ビルド済みバイナリ（推奨）

[Releases](https://github.com/funap/xvw/releases) ページから各OS向けの最新リリースをダウンロード:

- **macOS** (`.dmg`):
  1. `xvw-<version>-aarch64-apple-darwin.dmg` のダウンロード。
  2. ディスクイメージを開き、**`xvw.app`** を **Applications**（アプリケーション）フォルダへドラッグ＆ドロップ。
  
  > [!TIP]
  > **macOS: 「“xvw”は壊れているため開けません」と警告される場合**  
  > macOS Gatekeeperによる起動ブロック。オープンソース版のためApple開発者プログラムによる公証（Notarization）未取得によるものであり、バイナリ自体は安全かつ正常。  
  > 実行許可の手順（ターミナルでのコマンド実行）:  
  > ```bash
  > xattr -cr /Applications/xvw.app
  > ```
  > *(または Finder 上で `xvw.app` を **Controlキーを押しながらクリック**（右クリック）→「**開く**」を選択し、確認ダイアログで「**開く**」を実行)*

- **Linux** (`.deb` / `.rpm`):
  - **Debian / Ubuntu**: `sudo dpkg -i xvw_<version>_linux_amd64.deb`
  - **Fedora / RHEL**: `sudo rpm -i xvw-<version>-1.linux.x86_64.rpm`
- **Windows** (`.zip`):
  - `xvw-<version>-x86_64-pc-windows-msvc.zip` を解凍し、`xvw.exe` を実行。

#### ソースコードからのビルド

##### 前提条件

- **Rust**（2024 edition / 最新の安定版）  
  [rustup.rs](https://rustup.rs/) 経由でのインストール:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

##### ビルドおよび実行

```bash
# リポジトリのクローン
git clone https://github.com/funap/xvw.git
cd xvw

# ビルドおよび実行
cargo run --release
```

### コマンドラインでの使用方法

目的の解析ワークフローを直接起動可能なCLIインターフェース:

```bash
# 特定バイナリファイルのオープン
xvw path/to/binary_file.bin

# プロジェクトフォルダ / ワークスペースのオープン
xvw --folder path/to/folder

# ファイルオープンと同時にKaitai Structフォーマット定義を適用
xvw firmware.bin --ksy specs/firmware.ksy

# 2つのバイナリファイルの左右比較（Diff）
xvw --diff original.bin patched.bin

# 特定サイドバーパネルの初期表示指定
# (指定可能: files, strings, structure, inspector, map, checksum, bookmarks)
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
| **ファイルのオープン（ダイアログ）** | `Cmd + O` | `Ctrl + O` |
| **フォルダのオープン** | `Cmd + Shift + O` | `Ctrl + Shift + O` |
| **左サイドバーの表示切替** | `Cmd + B` | `Ctrl + B` |
| **エディタの分割（右 / 下）** | `Cmd + \` / `Cmd + Shift + D` | `Ctrl + \` / `Ctrl + Shift + D` |
| **タブの切り替え** | `Cmd + 1` .. `Cmd + 9` | `Ctrl + 1` .. `Ctrl + 9` |
| **Kaitai Struct（`.ksy`）の読み込み** | `Cmd + Shift + S` | `Ctrl + Shift + S` |
| **インライン構造体表示の切替** | `Cmd + Shift + V` | `Ctrl + Shift + V` |
| **Hexダンプとしてコピー** | `Cmd + Shift + C` | `Ctrl + Shift + C` |
| **設定** | `Cmd + ,` | `Ctrl + ,` |

---

## 🌐 対応文字コード

40種類以上の文字エンコーディングを標準サポート。バイナリデータのデコードおよび検索に対応:

- **Unicode & ASCII**: UTF-8, UTF-16 LE, UTF-16 BE, ASCII
- **日本語**: Shift-JIS (CP932 / Windows-31J), EUC-JP, ISO-2022-JP
- **中国語・韓国語**: GBK, GB18030, Big5, EUC-KR
- **ISO-8859 ファミリー**: ISO-8859-1 〜 ISO-8859-16（Latin 1–10、キリル文字、アラビア文字、ギリシャ文字、ヘブライ文字、ケルト文字など）
- **Windows コードページ**: Windows-1250 〜 Windows-1258
- **レガシー & Mac**: KOI8-R, KOI8-U, Mac OS Roman, IBM866

---

## 🤝 コントリビューション

Issueの報告や機能提案、Pull Requestなどのコントリビューションを歓迎:

- 🐛 **バグ報告・機能提案**: GitHubの [Issue](https://github.com/funap/xvw/issues) まで。
- 💡 **開発への貢献**: Pull Requestを随時受付。大規模なアーキテクチャ変更時は、事前のIssue確認やDiscussion開始を推奨。

---

## 📄 ライセンス

[MIT License](LICENSE) 準拠。

- テーマ: [gpui-kit](https://github.com/longbridge/gpui-kit)
- アイコン: [Lucide](https://lucide.dev)
