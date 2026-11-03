# Windows packaging (draft)

Build `berry.exe` for `x86_64-pc-windows-gnu`, then ship it next to a
`berryos.img`, `AAVMF_CODE.fd`, and (for a clean machine) the QEMU runtime
pack. Double-clicking `berry.exe` with no arguments opens the device shell.

QEMU is GPLv2 and must remain a separate process/runtime pack; do not link it.

The current `berry` binary is a console-subsystem build. Before shipping a
clean double-click experience, add `#![windows_subsystem = "windows"]` for
release builds and attach to the parent console when launched from a terminal
(`AttachConsole(ATTACH_PARENT_PROCESS)`); this is tracked as the Windows
packaging tail. For now, launch from a terminal:

```bat
berry.exe shell --image berryos.img
```
