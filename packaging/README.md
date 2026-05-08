# Packaging notes (v0.4 Rust rewrite)

## udev rule for `/dev/uinput`

By default `/dev/uinput` is mode `0600 root:root`, so the F9 Talk binary
can't write to it as a non-root user even if the user is in the `input`
group. We ship `debian/udev/99-f9-talk.rules` which downgrades it to
`0660 root:input`.

For now (M2 dev), install manually:

```sh
sudo cp packaging/debian/udev/99-f9-talk.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger /dev/uinput
ls -l /dev/uinput   # expect: crw-rw---- 1 root input ...
```

You also need to be a member of the `input` group:

```sh
sudo usermod -aG input "$USER"
# then log out and back in once
```

The M4 cargo-deb postinst script will do all three steps automatically.

## Why uinput (not xdotool / wtype / ydotool)

- `xdotool` is X11-only — broken on native Wayland windows.
- `wtype` works on Wayland but is blocked on GNOME (the compositor
  doesn't implement `virtual-keyboard-unstable-v1`).
- `ydotool` does what we need but adds a runtime daemon dep.
- Direct `/dev/uinput` write injects scancodes at the kernel layer:
  works on X11 + Wayland identically, no extra processes.
