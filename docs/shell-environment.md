# Shell Environment

When running scripts in Atuin Desktop, you might encounter issues where commands that work in your terminal don't work in a script block. This is usually because of differences between **interactive** and **non-interactive** shells. This page explains how shells work and how to ensure your scripts run correctly.

## Shells vs Terminal Emulators

First, it's important to understand the difference between a **shell** and a **terminal emulator**:

- A **shell** (like `zsh`, `bash`, or `fish`) is a program that interprets and executes commands. It handles things like your `PATH`, aliases, environment variables, and shell functions.

- A **terminal emulator** (like iTerm2, Terminal.app, Alacritty, or Kitty) is a graphical application that displays the shell's output and captures your keyboard input. It handles things like fonts, colors, window size, and rendering.

When you open your terminal application, it launches a shell process inside it. The terminal emulator is responsible for *displaying* what the shell outputs, while the shell is responsible for *executing* your commands.

!!! tip "Using custom fonts and icons"
    Both Script and Terminal blocks use a terminal emulator for rendering, so special fonts (like Nerd Fonts with icons), colors, and other visual formatting will work. To use a custom font, configure it in the app settings.

## Interactive vs Non-Interactive Shells

Shells can run in two modes:

### Interactive Shells

An **interactive shell** is what you get when you open a terminal. It:

- Displays a prompt and waits for you to type commands
- Loads your full shell configuration (`.zshrc`, `.bashrc`, etc.)
- Supports features like tab completion, command history, and aliases

### Non-Interactive Shells

A **non-interactive shell** runs a script or command without user interaction. It:

- Executes commands from a script file or string
- Loads only a minimal configuration (`.zshenv` for zsh, or nothing for bash by default)
- Doesn't display a prompt or wait for input

**Script blocks in Atuin Desktop run as non-interactive shells.** This is faster and more suitable for automation, but it means your full shell configuration isn't loaded.

## Configuration File Loading

Different shells load different configuration files depending on the mode:

### Zsh

| File | Interactive Login | Interactive Non-Login | Non-Interactive |
|------|:-----------------:|:---------------------:|:---------------:|
| `.zshenv` | Yes | Yes | Yes |
| `.zprofile` | Yes | No | No |
| `.zshrc` | Yes | Yes | No |
| `.zlogin` | Yes | No | No |

### Bash

| File | Interactive Login | Interactive Non-Login | Non-Interactive |
|------|:-----------------:|:---------------------:|:---------------:|
| `.bash_profile` | Yes | No | No |
| `.bashrc` | No | Yes | No |

!!! warning "The common mistake"
    Many users add `PATH` modifications to `.zshrc` or `.bashrc`. These files are **not loaded** when running non-interactive scripts. If you run `which node` in your terminal and it works, but a script block can't find `node`, this is almost certainly why.

## How Atuin Desktop Handles This

Atuin Desktop tries to help by capturing environment variables from an interactive shell at startup. When the app launches, it:

1. Opens an interactive shell session
2. Copies key environment variables (including `PATH`)
3. Uses these variables when running script blocks

This means that **tools available in your terminal when Atuin Desktop starts should also be available in script blocks**.

However, this approach has limitations:

- If you install new tools after launching Atuin Desktop, they won't be available until you restart the app
- If your shell configuration is complex or slow, the capture might not work correctly
- Some environment managers (like `nvm`, `pyenv`, or `rbenv`) set up their environment dynamically in ways that might not transfer

## Fixing PATH Issues

If commands aren't found in your script blocks, try these solutions:

### Solution 1: Restart Atuin Desktop

If you recently installed a tool, restart Atuin Desktop so it can capture your updated environment.

### Solution 2: Move PATH Configuration to .zshenv

For zsh users, move your `PATH` modifications from `.zshrc` to `.zshenv`:

```bash
# In ~/.zshenv (loaded for ALL shell types)
export PATH="$HOME/.local/bin:$PATH"
export PATH="/opt/homebrew/bin:$PATH"
```

### Solution 3: Use Absolute Paths

If you know where a tool is installed, use its absolute path:

```bash
# Instead of:
node script.js

# Use:
/opt/homebrew/bin/node script.js
```

### Solution 4: Source Your Configuration

You can explicitly source your shell configuration at the start of a script:

```bash
source ~/.zshrc
# Now run your commands
node script.js
```

!!! note
    This adds startup time to every script execution and may cause issues if your `.zshrc` contains interactive-only commands.

### Solution 5: Set PATH in the Script

For portability, set the required paths directly in your script:

```bash
export PATH="/opt/homebrew/bin:$HOME/.local/bin:$PATH"
node script.js
```

## Environment Managers

Tools like `nvm`, `pyenv`, `rbenv`, and `conda` often initialize themselves in `.zshrc` or `.bashrc`. To use them in script blocks:

### nvm (Node Version Manager)

```bash
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && source "$NVM_DIR/nvm.sh"
node --version
```

### pyenv

```bash
export PYENV_ROOT="$HOME/.pyenv"
export PATH="$PYENV_ROOT/bin:$PATH"
eval "$(pyenv init -)"
python --version
```

### rbenv

```bash
export PATH="$HOME/.rbenv/bin:$PATH"
eval "$(rbenv init -)"
ruby --version
```

### Homebrew (macOS)

```bash
eval "$(/opt/homebrew/bin/brew shellenv)"
# Now Homebrew-installed tools are available
```

## Using Terminal Blocks Instead

If you need full interactive shell features—including your complete environment, aliases, and formatted output—use a [Terminal block](blocks/executable/terminal.md) instead of a Script block. Terminal blocks run an interactive shell session with your full configuration loaded.

The tradeoff is that Terminal blocks:

- Are slower to start
- Cannot have their output captured as a variable (you must use `$ATUIN_OUTPUT_VARS`)
- Require an explicit `exit` command for serial execution to continue
