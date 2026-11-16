# Sorcerer

```
   ███████╗ ██████╗ ██████╗  ██████╗███████╗██████╗ ███████╗██████╗
   ██╔════╝██╔═══██╗██╔══██╗██╔════╝██╔════╝██╔══██╗██╔════╝██╔══██╗
   ███████╗██║   ██║██████╔╝██║     █████╗  ██████╔╝█████╗  ██████╔╝
   ╚════██║██║   ██║██╔══██╗██║     ██╔══╝  ██╔══██╗██╔══╝  ██╔══██╗
   ███████║╚██████╔╝██║  ██║╚██████╗███████╗██║  ██║███████╗██║  ██║
   ╚══════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝╚══════╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝
```

<div align="center">

[![CI](https://github.com/Panz-08/sorcerer/actions/workflows/ci.yml/badge.svg)](https://github.com/Panz-08/sorcerer/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-7C3AED.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-06B6D4?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-7C3AED)](https://github.com/Panz-08/sorcerer)

</div>

**Sorcerer** adalah konverter file dan dokumen serba bisa untuk terminal.
Binary-nya bernama **`sorce`**, tampilannya dibangun penuh dengan
[ratatui](https://ratatui.rs), dan proyek ini dibuat oleh **Panz-08**.

Sorcerer menggabungkan dua sisi:

- **TUI interaktif** — browser file, preview gambar/PDF/SVG/teks/video,
  pemilihan format, antrean konversi, dan progress bar.
- **CLI** — satu perintah untuk konversi tunggal, batch paralel, merge PDF,
  inspeksi metadata, pengecekan backend, dan shell completion.

---

## Daftar isi

- [Fitur](#fitur)
- [Format yang didukung](#format-yang-didukung)
- [Kebutuhan sistem](#kebutuhan-sistem)
- [Instalasi](#instalasi)
- [Mulai cepat](#mulai-cepat)
- [Panduan TUI](#panduan-tui)
- [Perintah CLI](#perintah-cli)
- [Contoh pemakaian](#contoh-pemakaian)
- [Opsi konversi](#opsi-konversi)
- [Cara kerja](#cara-kerja)
- [Struktur proyek](#struktur-proyek)
- [Troubleshooting](#troubleshooting)
- [Lisensi](#lisensi)

---

## Fitur

- **Full-screen TUI** dengan panel browser, preview, settings, dan queue.
- **Preview gambar** memakai Unicode half-block (`▀`) sehingga resolusi
  vertikal terminal menjadi dua kali lipat.
- **Preview PDF** halaman pertama, **preview SVG**, **preview teks**, dan
  **frame video** lewat ffmpeg.
- **Metadata file** langsung di panel settings: dimensi, jumlah halaman,
  durasi, codec, ukuran, dan waktu modifikasi.
- **44 format** dari gambar, vektor, dokumen kantor, teks, sampai media.
- **Batch paralel** dengan worker thread (jumlah worker bisa diatur).
- **Merge ke PDF** dari gambar, PDF, dokumen, atau teks.
- **PDF page selection**: `all`, `1`, `2-5`, `1,3,7-9`, `10-`.
- **Operasi gambar**: resize, rotate, flip, grayscale, background untuk alpha.
- **Output aman**: tidak menimpa file secara default; jika target sama dengan
  sumber, Sorcerer memakai nama `*-converted.ext`.
- **Pengecekan backend** lewat `sorce doctor`.
- **Shell completion** untuk bash, zsh, fish, PowerShell, dan Elvish.

---

## Format yang didukung

### Matriks konversi

| Sumber | Bisa menjadi |
| --- | --- |
| Gambar raster (PNG, JPEG, WebP, GIF, BMP, TIFF, ICO, TGA, PNM, QOI, HDR, EXR, Farbfeld, AVIF, HEIC, JXL) | Gambar raster lain, PDF |
| SVG | Gambar raster, PDF |
| PDF | Gambar raster, TXT, SVG, PDF (subset halaman) |
| Dokumen kantor (DOC, DOCX, ODT, RTF, XLS, XLSX, ODS, CSV, PPT, PPTX, ODP) | PDF, gambar raster, TXT/CSV, dokumen kantor lain, HTML |
| Teks (TXT, Markdown, HTML) | PDF, gambar raster, TXT/Markdown/HTML, DOCX/ODT |
| Video (MP4, WebM, MKV, MOV, AVI) | Video lain, GIF, frame gambar (PNG/JPEG/WebP/…) |
| Audio (MP3, WAV, OGG, FLAC, AAC, M4A, OPUS) | Audio lain |

### Daftar format

**Gambar / raster**

`PNG` · `JPEG` · `WebP` · `GIF` · `BMP` · `TIFF` · `ICO` · `TGA` · `PNM` ·
`QOI` · `HDR` · `EXR` · `Farbfeld` · `AVIF` · `HEIC` · `JPEG XL`

**Vektor & dokumen**

`SVG` · `PDF` · `DOC` · `DOCX` · `ODT` · `RTF` · `TXT` · `Markdown` · `HTML` ·
`XLSX` · `XLS` · `ODS` · `CSV` · `PPTX` · `PPT` · `ODP`

**Media**

`MP4` · `WebM` · `MKV` · `MOV` · `AVI` · `MP3` · `WAV` · `OGG` · `FLAC` ·
`AAC` · `M4A` · `OPUS`

Beberapa format membutuhkan backend eksternal. Lihat tabel di bawah dan jalankan
`sorce doctor` untuk melihat status di mesin kamu.

---

## Kebutuhan sistem

### Wajib

- **Rust 1.88+** dan Cargo (edition 2024).
- Linux, macOS, atau Windows. TUI paling nyaman di terminal modern yang
  mendukung Unicode dan warna 24-bit.

### Backend eksternal (opsional)

Sorcerer punya engine native untuk tugas-tugas umum. Backend berikut dipakai
untuk format atau operasi tertentu:

| Tool | Paket Debian/Ubuntu | Dipakai untuk |
| --- | --- | --- |
| Poppler | `poppler-utils` | render PDF, ambil teks, info PDF, merge/split |
| Ghostscript | `ghostscript` | fallback render PDF |
| LibreOffice | `libreoffice` | dokumen kantor ↔ PDF / format lain |
| ffmpeg & ffprobe | `ffmpeg` | video, audio, GIF, frame, fallback HEIC/JXL |
| MuPDF | `mupdf-tools` | fallback render PDF (`mutool`) |
| wkhtmltopdf | `wkhtmltopdf` | HTML → PDF dengan hasil terbaik |
| Chromium/Chrome | `chromium` | alternatif HTML → PDF |
| cwebp | `webp` | encoding WebP lossy via libwebp |
| pandoc | `pandoc` | konversi teks/dokumen tambahan |

Contoh memasang semuanya di Debian/Ubuntu:

```bash
sudo apt update
sudo apt install poppler-utils ghostscript libreoffice ffmpeg \
                 mupdf-tools webp pandoc wkhtmltopdf
```

Tanpa backend eksternal, konversi gambar dasar, SVG, pembuatan PDF dari
gambar/teks, serta TUI tetap berjalan.

---

## Instalasi

### Unduh binary rilis

Rilis siap pakai untuk Linux x86_64 tersedia di
[halaman Releases](https://github.com/Panz-08/sorcerer/releases/latest):

```bash
tar -xzf sorce-v0.1.0-linux-x86_64.tar.gz
sudo install -m 755 sorce /usr/local/bin/sorce
```

Cek checksum dengan `sha256sum -c SHA256SUMS` sebelum dipasang.

### Dari source (disarankan saat pengembangan)

```bash
cd sorcerer

# build release
cargo build --release

# jalankan langsung
./target/release/sorce

# atau pasang ke ~/.local/bin
./install.sh
```

`install.sh` memasang binary ke `${PREFIX:-$HOME/.local}/bin/sorce`.
Kalau belum ada di `PATH`, tambahkan:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Untuk instalasi sistem:

```bash
sudo PREFIX=/usr/local ./install.sh
```

### Dengan Cargo

```bash
cargo install --path .
```

Pastikan `~/.cargo/bin` ada di `PATH`.

### Cek instalasi

```bash
sorce --version
sorce doctor
```

---

## Mulai cepat

Buka TUI tanpa argumen:

```bash
sorce
```

Atau langsung mulai di folder tertentu / pilih file tertentu:

```bash
sorce tui ~/Pictures
sorce tui ~/Documents/laporan.pdf
```

Konversi cepat dari CLI:

```bash
# JPEG → PNG
sorce photo.jpg -o photo.png

# PNG → PDF
sorce poster.png -o poster.pdf

# PDF → PNG semua halaman
sorce scan.pdf --to png -o halaman

# PDF → PNG hanya halaman 1–3
sorce scan.pdf --to png -p 1-3 -o halaman --dpi 200

# WebP dengan kualitas 82
sorce photo.jpg --to webp -q 82 -o photo.webp

# Resize + grayscale
sorce photo.jpg --resize 1280x --grayscale -o kecil.jpg

# Batch satu folder, 8 worker
sorce batch ~/Pictures --to webp -q 82 -o converted -j 8

# Gabungkan gambar, PDF, dan teks menjadi satu PDF
sorce merge cover.png laporan.pdf catatan.txt -o bundel.pdf

# Ambil frame video pada detik ke-3
sorce video.mp4 --to png --seek 3 -o frame.png

# Video → GIF
sorce video.mp4 --to gif --fps 12 --seek 2 -o klip.gif
```

> `sorce photo.jpg -o photo.png` adalah shorthand untuk
> `sorce convert photo.jpg -o photo.png`. Keduanya sama-sama valid.

---

## Panduan TUI

Tampilan TUI terdiri dari:

```
┌──────────────────────────────────────────────────────────────┐
│ ◆ SORCERER   all-in-one converter       by Panz-08 · v0.1.0  │
├────────────────────────┬─────────────────────────────────────┤
│ Files                  │ Preview                             │
│  ..                    │  (gambar / PDF / teks / video)      │
│  ▸ folder/             │                                     │
│  ▣ photo.jpg           ├─────────────────────────────────────┤
│  ▤ laporan.pdf         │ Settings                            │
│                        │  Target · Output · Quality · DPI    │
├────────────────────────┴─────────────────────────────────────┤
│ Queue · 2 pending                                            │
│  ○ photo.jpg  → WEBP  ██████░░░░  60%  encoding image        │
├──────────────────────────────────────────────────────────────┤
│ q quit  Tab focus  Space mark  a add  c convert  t target     │
│ Status message                                               │
└──────────────────────────────────────────────────────────────┘
```

### Alur kerja

1. Navigasi folder dan tandai file dengan **Space**.
2. Pilih target dengan **t**, folder output dengan **o**.
3. Atur kualitas/DPI dengan **p** / **i** bila perlu.
4. Tekan **a** untuk memasukkan file bertanda ke queue.
5. Tekan **c** untuk mulai konversi.
6. Pindah ke queue dengan **Tab**, lihat progress, dan tekan **Enter**
   pada hasil yang sudah selesai untuk membuka preview output.

Kalau tidak ada file yang ditandai, **a** akan menambahkan file yang sedang
dipilih; jika yang dipilih adalah folder, semua file didukung di dalam folder
itu akan ditambahkan (tanpa rekursi).

### Keyboard

| Tombol | Fungsi |
| --- | --- |
| `q` / `Ctrl+C` | keluar |
| `Tab` | pindah fokus browser ↔ queue |
| `↑ ↓` / `k j` | navigasi |
| `PgUp` / `PgDn` | lompat 10 baris |
| `Home` / `End` | ke awal / akhir |
| `Enter` | buka folder, preview file, atau preview output queue |
| `←` / `Backspace` | naik ke folder induk |
| `Space` | tandai / lepas tanda file |
| `a` | tambah file bertanda (atau file/folder aktif) ke queue |
| `c` | konversi semua item pending di queue |
| `d` | hapus item queue yang dipilih |
| `x` | hapus semua tanda; di queue: bersihkan queue |
| `t` | pilih format target |
| `o` | atur folder output (kosong = folder sumber) |
| `f` | filter nama file |
| `g` | pergi ke folder |
| `h` | tampilkan / sembunyikan dotfile |
| `r` | refresh browser |
| `p` / `i` | edit kualitas / DPI |
| `w` / `n` | toggle overwrite / grayscale |
| `[` `]` | kualitas −5 / +5 |
| `,` `.` | DPI −25 / +25 |
| `1` `2` `3` `4` | target cepat: PNG / JPEG / WebP / PDF |
| `?` | bantuan |

### Preview

- **Gambar raster** — dimuat dan digambar dengan half-block. File sangat besar
  (> 96 MB) hanya menampilkan metadata agar TUI tetap responsif.
- **PDF** — halaman pertama dirender ke PNG sementara lewat `pdftoppm`,
  `pdftocairo`, atau Ghostscript.
- **SVG** — dirender native dengan resvg; DPI diturunkan otomatis untuk SVG
  raksasa.
- **Teks / Markdown / HTML / CSV** — 2.000 baris pertama.
- **Video** — frame pada detik ke-1 (fallback ke frame pertama) lewat ffmpeg.
- **Audio dan dokumen kantor** — metadata saja; konversi dulu ke PDF/PNG untuk
  melihat hasilnya.

Preview dimuat dengan jeda singkat (±110 ms) supaya navigasi cepat tidak
memblokir UI saat melewati file PDF atau gambar besar.

---

## Perintah CLI

```text
sorce [COMMAND]
```

Tanpa subcommand, Sorcerer membuka TUI. File/glob di posisi pertama juga
otomatis dianggap sebagai `convert`.

| Perintah | Alias | Fungsi |
| --- | --- | --- |
| `sorce convert <INPUT...>` | `c`, `conv` | konversi file, folder, atau glob |
| `sorce batch [PATH...]` | `b` | batch rekursif dengan struktur folder |
| `sorce merge <INPUT...> -o <FILE>` | | gabungkan ke satu PDF |
| `sorce inspect <FILE...>` | `info` | metadata file (teks atau JSON) |
| `sorce formats` | `list` | daftar format yang didukung |
| `sorce doctor` | `doc` | cek backend dan codec |
| `sorce tui [PATH]` | `ui` | buka TUI |
| `sorce completions <SHELL>` | `complete` | cetak shell completion |

Bantuan lengkap:

```bash
sorce --help
sorce convert --help
sorce batch --help
sorce merge --help
```

### `convert`

```bash
sorce convert INPUT... [OPTIONS]
```

- `INPUT` boleh file, folder, atau glob (`*.jpg`, `docs/**/*.pdf`).
- Folder diproses non-rekursif kecuali `--recursive` / `-r`.
- `--from png,jpg` membatasi format sumber.
- `--preserve-tree` mempertahankan struktur folder di output.

### `batch`

```bash
sorce batch [PATH...] [OPTIONS]
```

Default memproses folder saat ini secara rekursif dan mempertahankan struktur
folder. Gunakan `--flat` untuk meratakan output dan `--no-recursive` untuk
hanya folder teratas. `-j N` mengatur jumlah worker (0 = satu per CPU).

### `merge`

```bash
sorce merge INPUT... -o hasil.pdf [OPTIONS]
```

Menerima PDF, gambar, dokumen kantor, dan teks. Halaman digabung berurutan.
Opsi berguna: `--page-size fit|a4|a4-auto|letter`, `--margin 12`, `-q 92`,
`--dpi 150`, `-y`, `-n` (dry run).

### `inspect`

```bash
sorce inspect foto.jpg
sorce inspect --json hasil.pdf
sorce inspect -r ~/Documents
```

Menampilkan format, MIME, ukuran, dimensi, warna, jumlah halaman, durasi,
codec, FPS, jumlah baris/kata, dan waktu modifikasi.

### `formats`

```bash
sorce formats
sorce formats --json
```

### `doctor`

```bash
sorce doctor
```

Menampilkan status engine native, backend eksternal, dan encoder ffmpeg.
Gunakan ini pertama kali kalau ada format yang gagal.

### `completions`

```bash
# bash
sorce completions bash > ~/.local/share/bash-completion/completions/sorce

# zsh
sorce completions zsh > ~/.zsh/completions/_sorce

# fish
sorce completions fish > ~/.config/fish/completions/sorce.fish
```

---

## Contoh pemakaian

### Gambar

```bash
# Konversi banyak gambar sekaligus
sorce convert *.png --to webp -q 85 -o webp/

# Resize dengan mode cover, lalu simpan JPEG
sorce convert foto.jpg --to jpg --resize 1200x800 --fit cover -q 90 -o hasil.jpg

# Rotate 90°, flip horizontal, grayscale
sorce convert scan.png --rotate 90 --flip h --grayscale -o scan-fixed.png

# Background putih untuk PNG transparan → JPEG
sorce convert logo.png --to jpg --background white -o logo.jpg
```

### PDF

```bash
# Semua halaman → PNG 200 DPI
sorce convert laporan.pdf --to png --dpi 200 -o halaman/

# Halaman tertentu
sorce convert laporan.pdf --to png -p 1,3,5-7 -o pilihan/

# PDF → teks
sorce convert laporan.pdf --to txt -o laporan.txt

# PDF → SVG (satu SVG per halaman)
sorce convert diagram.pdf --to svg -o svg/

# Subset PDF: ambil halaman 2–4
sorce convert laporan.pdf --to pdf -p 2-4 -o ringkas.pdf

# Gabung beberapa PDF
sorce merge bab1.pdf bab2.pdf bab3.pdf -o buku.pdf
```

### Dokumen & teks

```bash
# Word → PDF
sorce convert proposal.docx -o proposal.pdf

# Word → PNG (halaman pertama)
sorce convert proposal.docx --to png -o proposal.png

# Markdown → PDF
sorce convert README.md -o README.pdf

# Markdown → HTML
sorce convert catatan.md --to html -o catatan.html

# TXT → DOCX (butuh LibreOffice)
sorce convert catatan.txt --to docx -o catatan.docx
```

### Media

```bash
# Video → MP4 H.264
sorce convert input.mov --to mp4 -o output.mp4

# Video → GIF, 12 FPS, mulai detik 3
sorce convert input.mp4 --to gif --fps 12 --seek 3 -o animasi.gif

# Ambil frame
sorce convert input.mp4 --to png --seek 5 -o frame.png

# Audio → MP3
sorce convert lagu.wav --to mp3 -q 90 -o lagu.mp3
```

### Batch

```bash
# Semua gambar di ~/Pictures → WebP, 8 worker
sorce batch ~/Pictures --to webp -q 82 -o ~/Pictures/webp -j 8

# Semua PDF dalam tree → PNG 150 DPI, struktur folder dipertahankan
sorce batch ~/Documents --from pdf --to png -o ~/Documents/png --dpi 150

# Dry run: lihat rencana tanpa menulis file
sorce convert *.png --to jpg -n
```

---

## Opsi konversi

| Opsi | Default | Keterangan |
| --- | --- | --- |
| `-t, --to <FORMAT>` | dari ekstensi output | format target |
| `-o, --output <PATH>` | folder sumber | file atau folder output |
| `-q, --quality <1-100>` | tergantung format | kualitas lossy |
| `--dpi <DPI>` | `150` | render PDF/SVG dan kepadatan halaman PDF |
| `-p, --pages <SPEC>` | `all` | halaman PDF: `1`, `2-5`, `1,3,7-9`, `10-` |
| `--resize <WxH>` | — | `1920x1080`, `1920x`, `x1080` |
| `--fit <MODE>` | `contain` | `contain`, `cover`, `fill` |
| `--rotate <DEG>` | `0` | `0`, `90`, `180`, `270` |
| `--flip <MODE>` | `none` | `none`, `h`, `v`, `both` |
| `-g, --grayscale` | off | konversi ke abu-abu |
| `--background <COLOR>` | `white` | untuk alpha: `white`, `black`, `#rrggbb`, `transparent` |
| `-y, --overwrite` | off | timpa file yang sudah ada |
| `--page-size <SIZE>` | `fit` | `fit`, `a4`, `a4-auto`, `letter` |
| `--margin <MM>` | `12` | margin PDF ukuran tetap |
| `--fps <FPS>` | `12` | FPS untuk video → GIF |
| `--seek <SECONDS>` | `0` | posisi awal video → gambar/GIF |
| `-j, --jobs <N>` | `0` | worker paralel; 0 = satu per CPU |
| `-r, --recursive` | off | rekursi folder |
| `--from <FORMATS>` | — | filter format sumber |
| `-n, --dry-run` | off | tampilkan rencana saja |
| `--quiet` | off | hanya error dan ringkasan |

Kualitas default mengikuti target: JPEG 90, WebP 88, AVIF/JXL 80. Format lossless
mengabaikan kualitas.

---

## Cara kerja

Sorcerer memilih *route* berdasarkan pasangan format sumber dan target:

- **Native Rust**
  - raster: crate `image` (JPEG, PNG, WebP, GIF, TIFF, BMP, ICO, TGA, PNM, QOI,
    HDR, EXR, Farbfeld, AVIF)
  - SVG: `resvg` + `usvg` + `tiny-skia`
  - PDF writer: implementasi sendiri tanpa dependensi (gambar, teks, halaman,
    alpha SMask)
  - teks/Markdown/HTML: `pulldown-cmark` dan konverter internal
- **Backend eksternal**
  - PDF render/text/merge/split: Poppler, Ghostscript, MuPDF
  - dokumen kantor: LibreOffice headless
  - media: ffmpeg / ffprobe
  - HTML → PDF: wkhtmltopdf, Chromium, LibreOffice, lalu fallback teks
- **Fallback**
  - jika sebuah encoder tidak tersedia di `image`, Sorcerer mencoba ffmpeg
    (misalnya WebP lossy atau JPEG XL)

Semua output ditulis lewat file sementara bila backend menghasilkan nama file
yang tidak bisa dikontrol, lalu dipindahkan ke tujuan akhir. Ini membuat nama
output tetap konsisten dan aman.

---

## Struktur proyek

```text
sorcerer/
├── Cargo.toml
├── Cargo.lock
├── CHANGELOG.md
├── LICENSE
├── README.md
├── install.sh
├── Makefile
├── src/
│   ├── main.rs            # entry point binary `sorce`
│   ├── cli.rs             # definisi clap
│   ├── lib.rs             # konstanta & modul publik
│   ├── core/
│   │   ├── convert.rs     # routing konversi
│   │   ├── image_ops.rs   # decode/transform/encode raster
│   │   ├── svg.rs         # render SVG
│   │   ├── pdf_write.rs   # PDF writer native
│   │   ├── pdf_render.rs  # render/subset/text PDF
│   │   ├── docs.rs        # LibreOffice, Markdown, HTML, teks
│   │   ├── media.rs       # ffmpeg video/audio
│   │   ├── merge.rs       # merge ke PDF
│   │   ├── batch.rs       # planning & eksekusi paralel
│   │   ├── inspect.rs     # metadata
│   │   ├── format.rs      # registry format & routing hints
│   │   ├── options.rs     # ConvertOptions
│   │   ├── backends.rs    # deteksi tool eksternal
│   │   ├── progress.rs    # progress callback
│   │   └── error.rs       # error types
│   ├── tui/
│   │   ├── app.rs         # state, event loop, queue, worker
│   │   ├── ui.rs          # layout & widget ratatui
│   │   ├── preview.rs     # preview gambar/PDF/SVG/teks/video
│   │   └── theme.rs       # warna, ikon, style
│   └── util/
│       ├── human.rs       # format byte, durasi, tanggal
│       ├── paths.rs       # glob, output naming, path helper
│       └── term.rs        # warna ANSI untuk CLI
├── tests/                 # integration tests untuk binary `sorce`
├── docs/                  # dokumentasi tambahan
└── assets/                # aset (opsional)
```

---

## Troubleshooting

### `sorce: command not found`

Pastikan lokasi binary ada di `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Atau jalankan langsung `./target/release/sorce`.

### TUI menolak dibuka

TUI membutuhkan stdout berupa terminal. Di dalam pipe, cron, atau CI, gunakan
mode CLI:

```bash
sorce convert input.jpg -o output.png
```

### `missing backend`

Jalankan:

```bash
sorce doctor
```

Contoh pesan dan solusinya:

- **PDF → PNG gagal** — pasang `poppler-utils` atau `ghostscript`.
- **DOCX → PDF gagal** — pasang LibreOffice (`soffice`).
- **Video/audio gagal** — pasang `ffmpeg` dan `ffprobe`.
- **HTML → PDF kurang rapi** — pasang `wkhtmltopdf` atau Chromium.
- **WebP lossy gagal** — pasang `cwebp` atau ffmpeg dengan encoder `libwebp`.

### File output sudah ada

Default-nya Sorcerer tidak menimpa file. Gunakan:

```bash
sorce convert foto.jpg -o foto.png -y
```

Di TUI, tekan `w` untuk mengaktifkan overwrite.

### Hasil PDF terpotong atau ukurannya aneh

Coba atur `--page-size` dan `--dpi`:

```bash
sorce convert gambar.png -o gambar.pdf --page-size a4 --dpi 200
sorce merge a.pdf b.pdf -o gabung.pdf --page-size a4-auto --margin 10
```

### Konversi lambat

- Gunakan `-j 0` (default) atau naikkan `-j` untuk batch.
- Turunkan `--dpi` untuk PDF → gambar.
- TUI memuat preview setelah jeda singkat; file besar tidak langsung dipreview.

### Warna terminal tidak muncul

Sorcerer mendeteksi `NO_COLOR`. Pastikan variabel itu tidak diset, dan terminal
mendukung true color. Di TUI, warna 24-bit dipakai langsung.

---

## Lisensi

MIT © **Panz-08**

Lihat [LICENSE](LICENSE) untuk teks lengkap.

---

Dibuat dengan Rust, ratatui, dan sedikit sihir. ✨
