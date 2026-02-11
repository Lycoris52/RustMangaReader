# <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">  RustMangaReader <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">

RustMangaReader is a high-performance, lightweight offline manga and comic viewer built in Rust. \
Designed specifically for the Windows, it focuses on providing a fluid, lag-free reading experience through preloading and native rendering.
### ⚡ Why MangaReader?

    Built for Speed: Uses a dual-buffer system to preload upcoming and previous pages in the background, ensuring near-instant page turns.
    Optimized for Windows: 
        ・Leverages Windows-native sorting (so "Page2" comes before "Page10")
        ・High-performance GPU rendering.
    Smart Scaling: Includes multiple resampling algorithms from Nearest Neighbor to Lanczos3 to make every scan look its best on your monitor.
    Tailored Reading: Supports Single Page, Double Page (Left-to-Right), and Double Page (Right-to-Left) modes, including a "Cover + Spreads" shift toggle (Odd/Even page).

### 📂 Supported Formats

RustMangaReader handles both compressed archives and raw file structures seamlessly.

    Archives	.zip, .cbz, .rar, .cbr
    Documents	.pdf
    Folders	Direct directory reading (reads images inside any folder)

### 🖼️ Image Extensions
RustMangaReader supports almost every modern image format, including high-efficiency codecs:

    Standard: .png, .jpg, .jpeg, .bmp, .tiff
    Web & Modern: .webp, .avif
    Legacy/Specific: .tga, .gif

### ⌨️ Controls & Customization

The app features a fully customizable keybinding system. By default, you can navigate using:

    Next/Prev Page: Arrow keys or Mouse Wheel (with smart-scroll locking).
    Fullscreen: Toggle for an immersive experience.
    Quick Settings: An integrated side panel to swap scaling methods, view modes, or rebind keys on the fly.

### 🛠️ Build Instruction

    [!IMPORTANT]
    Platform: Windows Only.
    Ensure you have the Rust toolchain installed.

    Clone the repository.

    Build the optimized executable:
    Bash

    cargo build --release

    The binary and your settings.json will be located in target/release/.

Feel free to contact me if you have any requests or found any bugs.

---
# <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">  RustMangaReader <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">

RustMangaReaderは、Rustで構築された高性能かつ軽量なオフラインマンガ・コミックビューアです。 \ 
Windows専用に設計されており、ダブルバファーとネイティブレンダリングによって、遅延のない滑らかな読書体験を提供することに特化しています。
### ⚡ 主な機能

    スピード重視: デュアルバッファシステムを採用し、背景で前後のページをプリロード。ページめくりがほぼ瞬時に完了します。
    Windowsに最適化: Windowsネイティブのソート順（「Page2」が「Page10」の前に正しく並ぶ）と、高性能なGPUレンダリングを活用しています。
    スマートスケーリング: Nearest Neighbor（最速）からLanczos3（高品質）まで、複数のリサンプリングアルゴリズムを搭載。どんなスキャン画像もモニターに合わせて美しく表示します。
    読書スタイルに合わせた閲覧: 単一ページ、見開き（左開き/右開き）モードをサポート。「表紙＋見開き」の切り替え（奇数/偶数ページ開始）も可能です。

### 📂 対応フォーマット

MangaReaderは、圧縮アーカイブと生のファイル構造の両方をシームレスに処理します。

    アーカイブ: .zip, .cbz, .rar, .cbr
    ドキュメント: .pdf
    フォルダ: ディレクトリを直接読み込み可能（フォルダ内の画像をスキャンします）

### 🖼️ 対応画像拡張子

高効率なコーデックを含む、ほぼすべての現代的な画像フォーマットをサポートしています。

    標準: .png, .jpg, .jpeg, .bmp, .tiff
    Web & モダン: .webp, .avif
    レガシー/特定用途: .tga, .gif

### ⌨️ 操作とカスタマイズ

アプリには完全にカスタマイズ可能なキーバインドシステムが搭載されています。 デフォルトの操作は以下の通りです：

    次/前のページ: 矢印キーまたはマウスホイール（スマートスクロールロック機能付き）。
    全画面表示: 没入感のある読書体験を切り替えます。
    クイック設定: 統合されたサイドパネルから、スケーリング方法や閲覧モードの変更、キーのリバインドが即座に行えます。

### 🛠️ ビルド方法

    [!IMPORTANT] プラットフォーム: Windows専用です。

    Rustツールチェーンがインストールされていることを確認してください。

    リポジトリをクローンします。

    最適化された実行ファイルをビルドします。
    Bash

    cargo build --release

    バイナリ（.exe）と settings.json は target/release/ フォルダ内に生成されます。