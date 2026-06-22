# brightvol

A tiny Windows tray daemon that intercepts the hardware **Volume Up** / **Volume
Down** keys and uses them to control your **screen brightness** instead of the
system volume.

It is a single self-contained `.exe`: no installer, no .NET, no runtime to
download, nothing to copy alongside it.

## What it does

- Intercepts the `VolumeUp` and `VolumeDown` media keys globally.
- Raises/lowers the **internal display** brightness (laptops/tablets) in ~10%
  steps instead of changing the volume.
- Lives in the system tray with a small menu:
  - **Enabled**: turn the interception on/off (so the volume keys work normally
    again when you need them).
  - **Start on login**: run brightvol automatically when you sign in.
  - **Exit**.
- On first launch it asks whether to start automatically on login and remembers
  your answer.

## Requirements

- Windows 10 or 11, 64-bit.
- A display whose brightness is controllable via WMI
  (`WmiMonitorBrightnessMethods`), i.e. typically a laptop/tablet internal
  panel. External monitors are not supported in this version.

## Download

Grab `brightvol.exe` from the latest [GitHub Actions build artifact][ci] or from
the [Releases][releases] page, then just run it. The icon appears in the system
tray.

[ci]: https://github.com/itskenny0/brightvol/actions
[releases]: https://github.com/itskenny0/brightvol/releases

## Configuration

State (enabled / autostart / first-run-done) is stored in
`%APPDATA%\brightvol\config.json`. Delete that file to see the first-run prompt
again.

## Building from source

Requires a Rust toolchain (stable) on Windows:

```sh
cargo build --release
# -> target/release/brightvol.exe
```

The release profile statically links the CRT, so the resulting binary has no
external runtime dependencies.

## License

[The Unlicense](LICENSE): public domain.
