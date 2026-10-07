# PT-D600 on Linux

> [!IMPORTANT]
> **Scope & Testing Disclaimer:**
> Everything modified, patched, and documented in this project was tested and verified **strictly on the Brother P-touch PT-D600 running on Linux**.

The PT-D600 is in the [USB device table](../crates/ptouch-core/src/device.rs)
as `04f9:2074`. This is the P-Touch tape printer, not the QL-600.
The editor and CLI send raster data directly over USB with libusb; a CUPS
queue or Brother's Windows editor is not needed.

## Build this fork

Install a current stable Rust toolchain using [rustup](https://rustup.rs/).

On Debian/Ubuntu:
```sh
sudo apt install build-essential pkg-config libusb-1.0-0-dev libudev-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libegl1 libgl1-mesa-dri \
  libssl-dev fonts-dejavu-core usbutils
```

On Arch Linux / CachyOS:
```sh
sudo pacman -S --needed base-devel git libusb systemd-libs libxkbcommon openssl
```

Build the binaries:
```sh
git clone https://github.com/wakuwakumiwaku/ptouch-rs-notstupid.git
cd ptouch-rs-notstupid
cargo +stable build --release --workspace --locked
```

The binaries are `target/release/ptouch-gui` and `target/release/ptouch`.
Upstream release packages do not contain this fork's changes.

### Desktop Integration & Installation (Optional)

Install binaries and the desktop launcher to `~/.local`:
```sh
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
ln -sf "$(pwd)/target/release/ptouch" ~/.local/bin/ptouch
ln -sf "$(pwd)/target/release/ptouch-gui" ~/.local/bin/ptouch-gui
cp data/io.github.vowstar.ptouch-gui.desktop ~/.local/share/applications/
cp data/io.github.vowstar.ptouch-gui.svg ~/.local/share/icons/hicolor/scalable/apps/
```

## Design a label before connecting the printer

```sh
./target/release/ptouch-gui
```

1. Click **Open Layout** and choose `examples/pt-d600-12mm.ptl`, or use
   **Add Text** for a new label.
2. Select the text element to edit its content and formatting. Set the tape
   width to match the cartridge you intend to use; the example is for 12 mm.
3. Check the live preview. **Save Layout** keeps an editable `.ptl` design;
   **Export Image** makes an image rather than sending anything to the printer.

The same design can be previewed from a shell without a printer:

```sh
./target/release/ptouch print --layout examples/pt-d600-12mm.ptl -o preview.png
./target/release/ptouch print --layout examples/pt-d600-12mm.ptl -o preview.jpg
```

PNG is preferable for crisp label artwork; JPEG is lossy. The example uses
180 dpi, the PT-D600's profile resolution. Do not confuse the CLI's `-w`
option (printable pixels) with the GUI's tape width in millimeters. A 12 mm
PT-D600 preview uses 76 printable pixels across the tape.

## Connect over USB

Power on the printer with a cartridge loaded and connect a data-capable USB
cable. Confirm that Linux can see it:

```sh
lsusb -d 04f9:2074
```

For a normal Linux desktop session, install the supplied access rule, reload
udev, then unplug and reconnect the printer:

```sh
sudo install -m 644 data/udev/20-usb-ptouch-permissions.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
```

Do not run the editor as root. Query the model and loaded tape first:

```sh
./target/release/ptouch info
```

In the editor, click **Refresh**, check the detected tape width, review the
preview, then click **Print**. These CLI commands actually consume tape:

```sh
./target/release/ptouch print "USB-C cables"
./target/release/ptouch print --layout examples/pt-d600-12mm.ptl

# Print a standalone QR code
./target/release/ptouch print -q "https://github.com/wakuwakumiwaku/ptouch-rs-notstupid"

# Print combined text and QR code
./target/release/ptouch print "Rack A1" -q "https://inventory.local/rack-a1"
```

Use the example's print command only with a matching 12 mm cartridge. If a job
fails, inspect the printer and label before retrying; a partially printed job
can jam the cutter mechanism.

### Disconnects or `usblp` driver conflicts

If the printer appears briefly in `dmesg` and immediately disconnects, the kernel's legacy `usblp` (USB line printer) module is claiming the device and resetting the USB controller. Blacklist it so userspace `libusb` can communicate without interference:

```sh
sudo modprobe -r usblp
echo "blacklist usblp" | sudo tee /etc/modprobe.d/blacklist-usblp.conf
```

### Headless sessions and WSL permissions

`TAG+="uaccess"` grants access through an active local desktop seat. For a
headless session without that grant, use a dedicated group rule instead of
making every USB device writable:

```sh
sudo groupadd -f plugdev
sudo usermod -aG plugdev "$USER"
```

Create `/etc/udev/rules.d/70-pt-d600-local.rules` containing this one line:

```udev
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="04f9", ATTR{idProduct}=="2074", GROUP="plugdev", MODE="0660"
```

Reload the rules, reconnect the printer, and start a new login session for
the group membership to take effect. This rule is limited to the PT-D600.

### USB from Windows into WSL 2

A Windows USB connection is not automatically visible inside WSL. Follow
[Microsoft's USB guide](https://learn.microsoft.com/en-us/windows/wsl/connect-usb)
to install `usbipd-win`. Keep a WSL terminal open. In PowerShell, identify the
printer's actual bus ID with `usbipd list`; do not reuse another device's ID.
Run `usbipd bind --busid <busid>` as administrator, then
`usbipd attach --wsl --busid <busid>` to attach it. Replace `<busid>` with the
ID you found. Back in WSL, repeat `lsusb -d 04f9:2074` and the permission checks
above. The GUI also needs WSLg or another graphical Linux session.

While attached to WSL, the printer is unavailable to Windows applications.
Use `usbipd detach --busid <busid>` to return it. This is separate from running
the native Windows binary; do not replace Windows drivers with Zadig merely
to use the Linux build in WSL.

## Tape Waste & Margin Mechanics on PT-D600

Brother P-Touch printers physically have approximately 25 mm (~1 inch) of distance
between the thermal printhead and the exit cutter blade. Because TZe cartridges
contain separate base tape, ink ribbon, and laminating film layers that only feed
forward, the printer cannot mechanically reverse tape.

When a cut is performed, the 25 mm of tape between the printhead and the cutter has
already moved past the printhead.

This project provides multiple ways to handle this in `ptouch-gui` and `ptouch-cli`:

1. **Auto Cut (Default)**:
   - Prints the label and cuts once at the end.
   - Does **not** snip an extra waste scrap before printing (`precut = false`).
2. **Chain Print / No Cut (Zero Waste for batches)**:
   - In `ptouch-gui`, toggle "Auto cut" to **"Chain print (no cut)"** (or pass `--chain` in CLI).
   - Subsequent labels print back-to-back with **0 mm wasted tape** between them.
   - When finished with your batch, click **"Feed & Cut"** in the toolbar.
3. **Multi-Copy Batching**:
   - Set **Copies** (in GUI or `-n` in CLI). All intermediate copies are automatically
     chained with zero waste between them, cutting only after the final copy.
4. **Trim Leader (Pre-cut)**:
   - Enable "Trim leader scrap (pre-cut)" in GUI (or `--precut` in CLI).
   - The printer will snip off the 25 mm leader before printing for symmetrical margins.
5. **Smart Automatic Pre-Trimming & Mixed Batches**:
   - In `ptouch-gui`, if label elements extend into the left prehead margin zone, the editor automatically detects and enables pre-trimming.
   - Batch queues seamlessly support mixed batches containing both standard zero-waste chain labels and pre-trimmed labels.

## Reliability changes in this fork

- Accumulate fragmented USB status replies and report cutter/error/power-off
  notifications instead of treating a short reply as a successful print.
- Refuse another job on a failed USB session until it is explicitly
  reinitialized. The existing best-effort behavior for a printer that sends
  no completion reply is retained; silence is not proof that a label printed.
- Category 3 hardware safety protection: canvas and controls are safely locked and dimmed out until the printer is connected and powered on.
- Emergency print cancellation button (`Cancel Print`) and live worker status indicators.
- Synchronized multi-label batch cutting to prevent buffer desync and cutter stalling.
- Pure-Rust 1-bit integer module QR code generation with quiet zones, scannable with zero thermal printhead subpixel blur.
- Support JPEG export, PNG fallback for unknown extensions, and finish image
  encoding before opening the destination so an encoder error does not
  truncate an existing file. This is not an atomic-write or disk-full guarantee.

The protocol regressions use synthetic transports and the real PT-D600
profile. They do not replace a hardware test. Check cartridge detection,
label alignment, cutting and error recovery on the actual printer before
relying on a batch run.

```sh
cargo +stable test --workspace --locked
cargo +stable fmt --all -- --check
cargo +stable clippy --workspace --locked -- -D warnings
```
