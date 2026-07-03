# SafeRecall

SafeRecall is a CLI tool to OCR and search for text in your photos.

## Features

- **Built-in OCR:** Uses the OS-native OCR engine where available (Apple Vision on macOS, Windows OCR on Windows) and falls back to an embedded [ocrs](https://github.com/robertknight/ocrs) engine everywhere else — no need to install external OCR engines like Tesseract.
- **Fast Fuzzy Search:** fzf-style fuzzy search through OCR results cached in a local SQLite database.
- **Static Binary:** Compiles to a single binary with everything included.

## Usage

```sh
recall "text to find" [directory]   # index new/changed images, then search
recall -g "text to find"            # search everything ever indexed
recall --cached "text to find"      # search the cache without re-scanning
recall --wipe                       # clear all cached OCR results
```

Directories are walked recursively (hidden files and `.gitignore`d paths are skipped), OCR runs in parallel (`-n` to limit threads), and results are cached — a file is only re-OCRed when its modification time changes.

## Credits and Licenses

This tool uses pre-trained OCR models from the [ocrs](https://github.com/robertknight/ocrs) project by Robert Knight:
- Repository: [Hugging Face](https://huggingface.co/robertknight/ocrs)
- Author: Robert Knight
- License: [Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)](https://creativecommons.org/licenses/by-sa/4.0/)

The models are used unchanged. No endorsement by the original author is implied.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for full attribution of the libraries used.
