# ptouch-rs-notstupid v0.9.4

Downloadable release of this fork, including the desktop GUI (`ptouch-gui`) and
CLI (`ptouch`).

## Changes since v0.9.3

- **QR codes & rendering:**
  - Always include a full 4-module quiet zone around QR codes (including next to
    other label elements).
  - Resolve fonts directly from the active `TextRenderer` database instead of
    rescanning system fonts on every render pass, and reuse a single
    `TextRenderer` across CSV batch rows.
  - Decode embedded SVG and SVGZ images when loading from memory (fixing SVG
    import in `ptouch-gui` and saved `.ptl` layouts).
  - Wrap balanced multi-line text using Unicode character counts rather than
    UTF-8 byte length.
- **Validation & CLI fixes:**
  - Reject duplicate CSV headers before generating or overwriting any labels.
  - Reject zero-DPI layouts, unsupported embedded document versions in `.ptb`
    batches, and non-finite or out-of-range margin/length/size/width values.
  - Report exported layout lengths using the saved design DPI, and use the
    connected printer's DPI when confining layout length.
  - Honor saved pre-trim settings on USB layouts and send `--precut` only on the
    first copy of a multi-copy chained print.
  - Report `PLite` and unsupported-raster errors during USB auto-discovery when
    no other supported printer is connected.
- **GUI fixes & polish:**
  - Fix a UTF-8 byte-slicing panic when deriving titles for multi-byte label
    text (umlauts, symbols, CJK).
  - Fix 3.5 mm tape selection in the setup modal (map to protocol width `4 mm` /
    `24 px`) and add `36 mm`.
  - Fix dark-mode text contrast across panels, remove missing-glyph boxes, stop
    left tape-width labels from clipping, show actual QR size for the active
    tape width, and align per-card batch tape estimates with the batch summary.
  - Use `.ptb`/`.ptl` filters for Open/Save Batch dialogs and prevent Linux
    fallback dialogs from popping up after cancelling a native file dialog.
  - Start fresh GUI sessions with a single clean label instead of hardcoded test
    entries (`examples/test-batch.ptb` remains available as an example).
- **Repo & packaging:**
  - Fix `flake.nix` source filtering to include bundled fonts in `data/fonts/`.
  - Rewrite `README.md` and update Linux documentation and issue templates for
    this fork.

## Downloads

- **Linux x86-64:** GUI and CLI binaries, Debian/Ubuntu `.deb`, Fedora/RHEL
  `.rpm`. Requires GLIBC 2.35 or newer and the `libudev` runtime.
- **Windows x86-64:** GUI and CLI executables (experimental).
- **macOS Apple Silicon:** GUI and CLI binaries plus the GUI `.app.zip` bundle
  (experimental).

Only the Brother PT-D600 under Linux has been hardware-tested for this fork.
Windows and macOS builds are experimental. Binaries and packages are unsigned,
so OS security prompts may appear.

Verify downloads against `SHA256SUMS`. Standalone Linux/macOS binaries need
`chmod +x <downloaded-file>` before launch. Source and licenses are available at
this tag and in GitHub's source archives; original `ptouch-rs` and
`ptouch-print` attribution is preserved in `NOTICE`, `LICENSE`, and
`LICENSE-MIT`.
