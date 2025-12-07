# Troubleshooting

This page covers common issues and frequently asked questions about Atuin Desktop.

## Scripts and Commands

### My script can't find a command that works in my terminal

This is usually because script blocks run in a **non-interactive shell**, which doesn't load your full shell configuration (like `.zshrc`). See our [Shell Environment](shell-environment.md) guide for a detailed explanation and solutions.

**Quick fixes:**

- Restart Atuin Desktop to pick up recently installed tools
- Move `PATH` configuration from `.zshrc` to `.zshenv`
- Use absolute paths (e.g., `/opt/homebrew/bin/node` instead of `node`)

### Fonts or icons don't display correctly

Both Script and Terminal blocks use a terminal emulator for rendering and support custom fonts. If special fonts (like Nerd Fonts) or icons aren't displaying correctly, make sure you've configured your preferred font in the app settings.

### nvm/pyenv/rbenv doesn't work in script blocks

These environment managers initialize in `.zshrc`, which isn't loaded for non-interactive shells. See the [Environment Managers](shell-environment.md#environment-managers) section for how to initialize them in your scripts.

## Application Issues

### The app doesn't launch, or launches with a blank window (Linux)

Tauri/WebkitGTK has known issues on Wayland and systems using proprietary Nvidia drivers. Try launching the app with one of these environment variables:

```bash
# Try these one at a time to see which works
__NV_DISABLE_EXPLICIT_SYNC=1 atuin-desktop
GDK_BACKEND=x11 atuin-desktop
WEBKIT_DISABLE_DMABUF_RENDERER=1 atuin-desktop
WEBKIT_DISABLE_COMPOSITING_MODE=1 atuin-desktop  # Note: increases CPU usage
```

Once you find one that works, you can add it to your `.desktop` file or shell configuration.

### Undo/redo is inconsistent or doesn't work

Currently, the main runbook document and each code editor have their own separate undo stacks. If you're editing code in a block and press undo, it will undo within that code editor—not in the main document. We plan to unify these in the future.

## Atuin Hub and Sync

### I host my own Atuin sync server. Can I use Atuin Desktop with it?

The Atuin CLI/sync server and Atuin Desktop are completely separate applications using different databases and backends. While Atuin Desktop can access your local Atuin CLI history for editor suggestions, your CLI history and Atuin Desktop data remain completely separate.

### Is Atuin Desktop open source? Can I run my own backend server?

[Atuin Desktop is open source](https://github.com/atuinsh/desktop). The backend for Atuin Desktop, called Atuin Hub, is more complex than the CLI sync server, and we don't currently offer an open-source version—but we're open to changing this in the future.

## Editing and Storage

### I'd like to edit Runbooks in an external editor or manage them via source control

By default, Atuin Runbooks are backed by CRDT-based documents, which cannot be edited in an external editor. However, **offline workspaces** store runbooks as flat files on your hard drive, and these can be edited externally—though the file format isn't optimized for this. We're working on a markdown-based file format to improve this experience.

Our intention with Atuin Hub is to make synchronizing, sharing, and editing Runbooks as frictionless as possible. If Hub isn't meeting your needs, or you prefer VCS-based runbook management, we'd love to hear from you on [our forum](https://forum.atuin.sh) or [Discord](https://discord.gg/jR3tfchVvW).

## Getting Help

If your issue isn't covered here:

- Check [our forum](https://forum.atuin.sh) for community discussions
- Join [our Discord](https://discord.gg/jR3tfchVvW) for real-time help
- [Open an issue on GitHub](https://github.com/atuinsh/desktop/issues) for bugs or feature requests
