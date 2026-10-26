# ptouch-rs-notstupid v0.9.1

First downloadable release of the customized fork, including its GUI designer
and CLI. This is the 0.9.x fork, not upstream ptouch-rs 0.14.1.

## Changes since v0.9.0

- Preserve exact whitespace in nonblank QR payloads.
- Composite transparent images on white before scaling and binarization, so
  black artwork remains visible in imported images and saved layouts.
- Add version-checked release builds, Linux installer metadata for this fork,
  and SHA256SUMS.

## Downloads

- Linux x86-64: GUI and CLI binaries, Debian/Ubuntu .deb, Fedora/RHEL .rpm.
- Windows x86-64: GUI and CLI executables.
- macOS Apple Silicon: GUI and CLI binaries plus the GUI .app.zip bundle.

Only the Brother PT-D600 under Linux has been hardware-tested for this fork.
Windows and macOS builds are experimental, not claims of printer compatibility.
Binaries and packages are not signed with a trusted platform developer
certificate; security prompts may appear.

Check downloads against SHA256SUMS. Standalone Linux/macOS binaries need
`chmod +x <downloaded-file>` before launch. Corresponding source and licenses
are available at this release tag and in GitHub's source archives. Original
ptouch-rs and ptouch-print attribution is preserved in NOTICE and the license
files.
