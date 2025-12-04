# Markdown Render

The Markdown Render block displays variable content as rendered markdown. Useful for displaying formatted output from scripts, API responses, or any variable that contains markdown text.

## Configuration

### Variable Name

The name of the variable containing markdown content to render. Supports template syntax:

```handlebars
{{var.release_notes}}
```

### Max Lines

Control the height of the content area by setting the maximum number of visible lines. Content exceeding this limit becomes scrollable. Default is 12 lines.

## Features

### Collapse/Expand

Toggle between a compact collapsed view and full expanded view to manage screen space. When collapsed, a gradient fade indicates more content is available.

### Fullscreen Mode

Open the rendered markdown in a fullscreen modal for easier reading of long content. Press `Escape` or click outside to close.

### Selectable Text

All rendered content is fully selectable and copyable, making it easy to extract information from the output.

## Example Workflow

A common pattern is using a Script block to generate markdown content, then displaying it with a Markdown Render block:

**Script Block:**
```bash
# Fetch release notes and output as markdown
gh release view --json body --jq '.body'
```

Save the output to a variable (e.g., `release_notes`), then reference it in your Markdown Render block.

**Markdown Render Block:**
- Variable: `release_notes`
- Max Lines: `20`

## Supported Markdown

The block supports GitHub Flavored Markdown (GFM), including:

- Headers and paragraphs
- **Bold**, *italic*, and ~~strikethrough~~ text
- Ordered and unordered lists
- Code blocks with syntax highlighting
- Tables
- Links and images
- Blockquotes
- Task lists

## View Mode vs Edit Mode

In **edit mode**, you can configure the variable name and line count. In **view mode** (when running the runbook), only the variable name and rendered content are shown for a cleaner experience.
