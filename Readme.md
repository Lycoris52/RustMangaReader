[日本語の説明はこちら](#japanese-explanation)

# <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">  RustMangaReader <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">

RustMangaReader is a high-performance, lightweight **offline** manga and comic viewer built in Rust. \
Designed specifically for the Windows, it focuses on providing a fluid, lag-free reading experience through preloading and native rendering.

# <img src="./src/assets/mangaviewer.png" alt="app image" width="500">

### ⚡ Key Features

* **Built for Speed**: Uses a dual-buffer system to preload upcoming and previous pages in the background, ensuring near-instant page turns.
* **Optimized for Windows**: 
    * Leverages **Windows-native sorting** (so "Page2" comes before "Page10")
    * **High-performance** GPU rendering.
    * **No Zip extraction required** RustMangaReader reads directly from compressed files saving disk space without sacrificing speed.
* **Smart Scaling**: Includes multiple resampling algorithms from Nearest Neighbor to Lanczos3 to make every scan look its best on your monitor.
* **Tailored Reading**: Supports Single Page, Double Page (Left-to-Right), and Double Page (Right-to-Left) modes, including a "Cover + Spreads" shift toggle (Odd/Even page).

### 🎁 Free & Open Feedback

MangaReader is a completely free application! \
I want to make it the best it can be for the community. If you encounter any bugs, have performance issues, or want to request a new feature, please feel free to contact me! You can reach out by:

* Opening an Issue here on GitHub.
* Contact me via [Huggingface](https://huggingface.co/Lycoris53) msg

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

    Full Keyboard controls: All key controls such as next/prev page, next zip file, next folder is bindable.
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

### ⚠️ Disclaimer & Responsibility

RustMangaReader is strictly a viewing utility. 
> * **No Content Provided**: This software does not provide, host, or link to any manga, comics, or archive download sources.
> * **User Responsibility**: Users are solely responsible for the content they open within the application.
> * **No Liability**: The developer takes no responsibility for any copyright infringements or legal issues arising from the user's choice of reading material or download sources.

### Third-Party Licenses 
This software uses Pdfium for PDF rendering.

    Pdfium is Copyright 2014 Google Inc. All rights reserved.
    Licensed under the Apache License, Version 2.0 (the "License"). You may obtain a copy of the License at: http://www.apache.org/licenses/LICENSE-2.0

Feel free to contact me if you have any requests or found any bugs.

---
<a name="japanese-explanation"></a>
# <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">  RustMangaReader <img src="./src/assets/icon128.ico" alt="An example image" width="32" height="32">

RustMangaReaderは、Rustで構築された高性能かつ軽量なオフラインマンガ・コミックビューアです。   
Windows専用に設計されており、ダブルバファーとネイティブレンダリングによって、遅延のない滑らかな読書体験を提供することに特化しています。
### ⚡ 主な機能

* **スピード重視**: デュアルバッファシステムを採用し、背景で前後のページをプリロード。ページめくりがほぼ瞬時に完了します。
* **Windowsに最適化**: Windowsネイティブのソート順（「Page2」が「Page10」の前に正しく並ぶ）と、高性能なGPUレンダリングを活用しています。
* **スマートスケーリング**: Nearest Neighbor（最速）からLanczos3（高品質）まで、複数のリサンプリングアルゴリズムを搭載。どんなスキャン画像もモニターに合わせて美しく表示します。
* **読書スタイルに合わせた閲覧**: 単一ページ、見開き（左開き/右開き）モードをサポート。「表紙＋見開き」の切り替え（奇数/偶数ページ開始）も可能です。
* **アーカイブの展開不要**: 圧縮ファイルから直接読み込み（オンザフライ読み込み）を行うため、ストレージを消費せず、かつ高速な動作を実現しています。

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

    フルキーボード操作: ページ送り、次のZIPファイル、次のフォルダ移動など、すべての操作を自由にキー割り当て（バインド）可能です。
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

### 🎁 完全無料・フィードバック大歓迎

MangaReaderは、完全に無料で利用できるアプリケーションです。

より良いアプリにしていきたいと考えておりますので、もしバグを見つけたり、動作が重いと感じたり、追加してほしい機能のアイデアがありましたら、ぜひお気軽にご連絡ください！ 連絡先は以下の通りです：

* GitHub の Issue を作成
* [Huggingface](https://huggingface.co/Lycoris53) などでのメッセージ

### ⚠️ 注意事項

MangaReaderは、あくまで閲覧専用のユーティリティソフトです。

* **コンテンツの提供について**: 本アプリは、マンガやコミックなどのコンテンツ、およびアーカイブのダウンロードソースを一切提供・ホスト・推奨することはありません。
* **ユーザーの責任**: アプリ内で開くコンテンツについては、すべてユーザーご自身の責任において管理・利用してください。
* **責任の制限**: ユーザーが使用する閲覧素材やダウンロード元に起因する著作権侵害や法的トラブルについて、開発者は一切の責任を負いません。
  

### サードパーティライセンス 
本ソフトウェアでは、PDFのレンダリングに Pdfium を使用しています。

    Pdfium は Google Inc. が著作権を有しています (Copyright 2014 Google Inc.)。
    本ライブラリは Apache License, Version 2.0 に基づいて提供されています。ライセンスの詳細は以下をご確認ください：http://www.apache.org/licenses/LICENSE-2.0