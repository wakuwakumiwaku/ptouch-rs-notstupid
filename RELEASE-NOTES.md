# ptouch-rs-notstupid v0.9.3

Downloadable release of the customized fork, including its GUI designer
and CLI. This is the 0.9.x fork, not upstream ptouch-rs 0.14.1.

## Changes since v0.9.0

- Preserve exact whitespace in nonblank QR payloads.
- Composite transparent images on white before scaling and binarization, so
  black artwork remains visible in imported images and saved layouts.
- Prefer exact font-family names over partial matches, preventing macOS
  system families containing "Interface" from taking the place of Inter.
- Add version-checked release builds, Linux installer metadata for this fork,
  and SHA256SUMS.
- Build Linux assets on pinned Ubuntu 22.04 and reject GLIBC requirements above
  2.35 before upload, instead of inheriting a newer ABI from ubuntu-latest.

## Downloads

- Linux x86-64: GUI and CLI binaries, Debian/Ubuntu .deb, Fedora/RHEL .rpm.
  Requires GLIBC 2.35 or newer and the libudev runtime.
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
