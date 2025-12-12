# Directory

This contextual block allows you to set the directory that all subsequent blocks should execute within.

## Path Types

The directory block supports three types of paths:

1. **Absolute paths**: Start with `/` (e.g., `/Users/username/project`)
2. **Home-relative paths**: Start with `~` (e.g., `~/Documents`)
3. **Relative paths**: Any other path (e.g., `../sibling`, `subfolder`)

Relative paths are resolved relative to:
- The previous directory block, if one exists
- The home directory (`~`) if no previous directory block exists

## Template Variables

You can use template variables in directory paths:

- `{{ workspace.root }}`: The absolute path to the workspace root (where `atuin.toml` is located). This is automatically looked up from the runbook's workspace and resolves to an empty string in online workspaces.
- Any custom variables defined in var blocks

## Examples

```
# Absolute path
/Users/username/my-project

# Home-relative path
~/projects/my-app

# Relative to workspace root
{{ workspace.root }}/src

# Relative to previous directory
../sibling-directory
```
