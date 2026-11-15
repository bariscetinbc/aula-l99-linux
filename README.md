# AULA L99 Linux app

Unofficial Linux desktop controls for the AULA L99 keyboard. Licensed under GNU GPL version 2 or later (GPL-2.0-or-later);
see [LICENSE](LICENSE) and [credits](THIRD_PARTY.md).

## Build and run on Linux

Install stable Rust, a C compiler, X11/OpenGL development libraries, Python 3
and Pillow. On Ubuntu: `sudo apt install build-essential libx11-dev libxi-dev
libxcursor-dev libxrandr-dev libgl1-mesa-dev python3 python3-pil policykit-1`.
From this folder run:

```sh
cargo test --locked
python3 -m unittest discover -s src -p 'test_*.py'
cargo build --release --locked
./target/release/aula_l99_linux
```

Run as your normal desktop user. Real device behavior requires the wired L99;
the build and tests alone do not establish hardware compatibility.


## CachyOS udev setup

Run the install command from the repository root (`devicedriver-2`):

```sh
sh app/install-permissions.sh
```

If your terminal is already inside `app/`, run `sh install-permissions.sh`.
The script asks for your administrator password, installs `70-aula-l99.rules`,
removes the previous `90-aula-l99.rules`, reloads udev and triggers the connected
HID and LCD serial devices. Then try Live data again; if needed, unplug and reconnect the keyboard.

The old `90-` filename ran after systemd's `73-seat-late.rules`, too late for
the `uaccess` tag to grant access. The `70-` rule grants the active local desktop
user access to the L99 live-data interface (03) and the EEEF:268A LCD serial port, without making it writable
by everyone. A remote or inactive session may not receive desktop-user access.

## Panel uploads and live readings

Touchscreen → Choose image or GIF → choose Photo frame, Wallpaper, or Customized
animation → Send to LCD. Python 3 and Pillow are required for encoding (the
app reports a missing dependency). The serial node is selected by EEEF:268A,
not by the first ttyACM port. Uploads run in a worker and require acknowledgments.
The serial port must be accessible to the desktop user. Select the matching
page on the LCD itself; USB uploads do not select its screensaver. Restart the
panel if it caches the old image. GIFs use a uniform frame delay and at most 200
frames; colors are quantized to the panel's supported palette.

GPU readings use nvidia-smi or DRM gpu_busy_percent and hwmon temperature.
Unavailable readings are shown as unavailable and sent as zero; zero CPU
and GPU temperatures remain valid wire values. The monitor packet's four
hardware fields are sent in Celsius to offsets 13–16 every second.

Touchscreen → Digital clock + live data starts the clock/monitor stream. On the
physical LCD select Home → Real-time Data; that page already has a digital clock.
The app also shows a digital preview with CPU/GPU numbers and bars from the same
samples. The on-board Customized animation clock cannot be redesigned over the
known USB protocol: GIFs loop stored frames and cannot keep live time or metrics.

The small thermometer beside humidity is the air/weather field (byte 17), not
CPU temperature (byte 14). **Show CPU temperature in the LCD thermometer (°C)**
is enabled by default and mirrors the selected CPU sensor into that field, while
retaining the normal CPU/GPU fields. This makes that thermometer a CPU reading;
it is no longer outdoor weather. Disable the option to leave the weather field
unset. Humidity remains unset (the firmware displays 0%). Missing CPU temperature
still sends zero; the desktop preview explicitly says unavailable. The wire uses
Celsius; no undocumented Fahrenheit flag is sent. Physical LCD rendering still
needs verification on the connected keyboard.
LCD brightness and automatic screensaver switching remain unsupported: their
USB commands are unknown. Adjust brightness and screensaver on the LCD.

Protocol references (independent implementation in src/panel.py):
https://github.com/gavindi/Aula_L99_Linux/blob/main/tools/aula_l99_screen/README.md
https://github.com/gavindi/Aula_L99_Linux/blob/main/tools/aula_l99_hacky/re_notes/system_monitor_block.md

RGB spectrum is available on Lighting; it selects the built-in full-color Spectrum effect.
Turn it off to apply the selected effect with a single color. HID replies are validated by matching the command and checking acknowledgement
bit 0, as in the reference Linux implementation. Both 01 and FF carry that bit;
replies with the bit clear or a different command remain errors.
Preview runs locally and never opens the serial port. Pillow deprecation warnings
are avoided using get_flattened_data on recent versions. After this update, rerun
`sh app/install-permissions.sh` once to install the added LCD serial access rule.
Temperature sources depend on Linux drivers; missing CPU temperatures now show
as unavailable instead of a misleading zero. No USB LCD brightness command is
known, so its visible slider is disabled; use the physical screen settings.

## Key mapping

For Turkish Q typing, enable **Shift + 2 → @ (Turkish Q)** and click
**Apply mappings…**. Either Shift key works. The helper emits a complete AltGr+Q
character and restores Shift immediately, so AltGr cannot spill into Enter.
Plain 2 and Ctrl/Alt/Windows shortcuts remain unchanged. Saved settings using the
previous Ctrl+Alt+Q workaround migrate to this option; apply again to activate.
Use the Turkish Q desktop layout; US layouts already provide Shift+2 natively.

Key Mapping replaces the individual key color editor. Select a physical key,
add a mapping, then choose AltGr, @, another key, or Disabled. The quick preset
maps Right Ctrl to AltGr. For @, select the shortcut used by your desktop layout:
AltGr+Q (for example Turkish/German) or Shift+2 (US). Fn is handled in keyboard
firmware and normally does not send a Linux input event, so Fn mappings are not
available; Right Ctrl → AltGr provides a usable alternative.

Apply mappings opens Linux's administrator authentication dialog. The included
Python helper uses evdev/uinput without extra packages and works on Wayland and
X11. It targets only the wired 0C45:800A keyboard; Python 3, PolicyKit and
/dev/uinput must be available. Stop or close the app to release the keyboard
and restore normal input. Saved mappings can be applied next time; they do not
change keyboard firmware. No administrator action happens on startup.

The Touchscreen temperature selector lists readable Linux hwmon sensors and
can save a choice. Automatic prefers CPU die/package over control and core
readings. Missing or invalid readings remain unavailable. Select a CPU sensor,
not a GPU, memory or disk sensor, when overriding the automatic choice.

## Security review (2026-11-14)

The desktop app is Rust (eframe/egui). Embedded Python 3 helpers handle LCD
image encoding/serial uploads and the explicitly authenticated evdev remapper.
The app itself should run as your normal desktop user, never with sudo.

This review covered the app source, helper scripts, device access rules and
installer. Helper processes now use the system Python with isolated mode (-I),
which excludes the working directory, PYTHONPATH and user site packages from
imports. The privileged mapping launcher also uses absolute executable paths.
Mapping payloads are validated before privileged devices are opened. Settings
loads are capped at 64 KiB and indices/ranges are validated before UI or hardware
use. Images above Pillow's 16 million pixel safety threshold are rejected before
decoding to limit decompression memory usage. The existing active-user-only udev
access rules and explicit PolicyKit authentication are retained.

No review proves the absence of all vulnerabilities. cargo-audit is not installed
in this environment, and the OSV dependency check could not reach its service
(DNS/network unavailable), so a current full dependency advisory audit remains
outstanding. RustSec documents Cargo.lock auditing at https://rustsec.org/.
Hardware behavior, hostile USB firmware, the operating system, Python/Pillow and
PolicyKit installations are outside what these source checks establish. Keep
system packages current. No privileged helper or hardware write was run during
this review.

## Follow-up security review (2026-11-15)

Reviewed Rust process launches, settings validation, HID bounds, Python media
handling, the privileged remapper, and udev/installer permissions. The remapper
now refuses symlinked device nodes and verifies the opened input device vendor
and product with EVIOCGID before grabbing it. Isolated Python, explicit PolicyKit
authentication and local-user device permissions remain in place. Automated
regressions cover Shift+2, repeat, either Shift, release order and Enter.
This source review cannot guarantee zero vulnerabilities; the dependency advisory
audit and physical-keyboard validation remain outstanding as described above.

## Firmware and LCD feature guide

The sidebar now has Firmware Update and LCD guide pages. The guide lists available
LCD functions, setup instructions, sensor dependencies and missing functionality.
Firmware Update describes separate keyboard/screen updates in the original driver.
Original executable strings reference firmware/L99 ENV1.03.exe and L99 ISP V1.23.exe;
the English vendor language file names Keyboard Upgrade and Screen Upgrade.
These filenames do not establish installed device versions. Linux firmware flashing,
version detection and recovery are not implemented; disabled controls state this.
Use the official Windows driver on Windows for firmware updates for the exact device.

## Windows, Mac and official resources

Software & manuals links the official Windows driver page:
https://aulagear.com/blogs/software/aula-l99-driver
This Rust build remains Linux-only (Linux HID, sysfs, evdev and serial helpers).
No Windows or macOS binary of this implementation is supplied. No verified Mac
configuration driver was found; advanced Mac controls/downloads are not offered.
Use the keyboard manual for its physical controls and connection modes:
https://aulagear.com/cdn/shop/files/AULA_L99_Manual.pdf?v=18380816096559756908

Firmware Update now links the supplied AULA guide rather than disabled flash
buttons: https://aulastar.com/announcement/788.html . It requires Windows,
separate keyboard and screen tools, the exact device package and uninterrupted
wired USB. The guide is not itself a firmware binary. This app does not flash
firmware or detect installed firmware versions. Do not infer installed firmware
from bundled updater filenames.

## Portable desktop build preparation (2026-11-15)

The Rust UI now separates Linux HID calls from portable desktop code and uses
native configuration folders on Windows and macOS. Windows capture mode's
compile error was removed. Keyboard layout and calculator preview are preserved.
These are desktop preview ports: USB controls, remapping, sensors and LCD media
helpers still require Linux. They are not full native device-driver ports.
The existing Linux helpers remain Python; this is not an all-Rust rewrite.
The Windows/Mac statements earlier in this file describe the earlier Linux release.

On Windows, install the Rust MSVC toolchain and Visual Studio C++ Build Tools,
then run `powershell -File scripts/build-windows.ps1` from app/. It tests and
builds `dist/AULA-L99.exe`. On a Mac with Rust and Xcode command-line tools,
run `sh scripts/build-macos.sh` to test/build `dist/AULA L99.app` for that Mac's
architecture. Releases are unsigned; signing/notarization is not configured.
No Windows or Mac binary has been built or tested on this Linux host; only the
Linux target is installed and registry DNS is unavailable.

Run `sh scripts/security-check.sh` with cargo-audit installed and network access
as a release gate. It fails if the audit tool/database is unavailable or warnings
are reported. No vulnerabilities are suppressed. This environment cannot run a
current advisory audit, so there is no zero-known-vulnerability certification.
Even a clean advisory scan cannot establish zero security vulnerabilities.
