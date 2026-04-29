# Markdown Render Fixture

Use this file to test the `markdown_render` block against the Markdown + GFM syntax that the current renderer should support.

## Headings

# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6

## Paragraphs

This is a normal paragraph with enough text to check spacing, wrapping, and line height in the rendered output.

This paragraph includes a
soft line break inside the same paragraph.

This line ends with two spaces  
so the next line should render as a hard break.

## Emphasis

This text includes *italic*, **bold**, ***bold italic***, ~~strikethrough~~, and `inline code`.

You can also mix them together: **bold with `inline code` inside**, *italic with a [link](https://example.com)*, and ~~strikethrough with **bold**~~.

## Links

Inline link: [Atuin](https://atuin.sh)

Autolink: <https://example.com/docs/getting-started>

Bare URL literal: https://example.com/releases/latest

Email autolink literal: support@example.com

Reference-style link: [Renderer docs][renderer-docs]

[renderer-docs]: https://example.com/markdown-render

## Blockquotes

> This is a simple blockquote.

> A blockquote can contain multiple paragraphs.
>
> It can also contain other Markdown:
> - a list item
> - another list item
>
> And a nested quote:
> > Nested quote content

## Lists

Unordered list:

- First item
- Second item
- Third item

Ordered list:

1. First ordered item
2. Second ordered item
3. Third ordered item

Nested lists:

- Parent item
  - Nested child item
  - Another nested child item
- Second parent item

1. Ordered parent
   1. Nested ordered child
   2. Another nested ordered child
2. Second ordered parent

## Task Lists

- [x] Completed task
- [ ] Incomplete task
- [x] Completed task with `inline code`
- [ ] Incomplete task with a [link](https://example.com)

## Code

Inline code example: `const answer = 42;`

Fenced code block with language:

```ts
type User = {
  id: string;
  name: string;
  active: boolean;
};

const user: User = {
  id: "u_123",
  name: "Taylor",
  active: true,
};

console.log(user.name);
```

Fenced code block without language:

```
Plain fenced code block
with multiple lines
and no language hint.
```

Indented code block:

    SELECT id, name
    FROM users
    WHERE active = true
    ORDER BY name ASC;

## Tables

Simple table:

| Column 1 | Column 2 |
| --- | --- |
| Value 1 | Value 2 |
| Value 3 | Value 4 |

Alignment table:

| Left | Right | Center |
| --- | ---: | :---: |
| left text | 123 | centered |
| more left text | 4567 | more centered |

Table with inline formatting:

| Syntax | Example | Notes |
| --- | --- | --- |
| Bold | **strong** | Should keep emphasis |
| Italic | *emphasis* | Should keep italics |
| Code | `npm test` | Should keep inline code |
| Link | [Example](https://example.com) | Should stay clickable |

## Horizontal Rules

---

Content after the first rule.

***

Content after the second rule.

___

## Images

Image from the app public directory:

![Vite Logo](/vite.svg)

Linked image:

[![Tauri Logo](/tauri.svg)](https://tauri.app)

## Footnotes

Here is a statement with a footnote.[^note-one]

Here is another footnote reference.[^note-two]

[^note-one]: This is the first footnote.
[^note-two]: This footnote includes **formatting**, `inline code`, and a [link](https://example.com).

## Escaping

\*This should not be italic\*

\[This should not become a link\](https://example.com)

\# This should not become a heading

## Raw HTML

<details>
  <summary>Expandable HTML block</summary>
  <p>This content is written in raw HTML inside the Markdown document.</p>
</details>

<kbd>Ctrl</kbd> + <kbd>K</kbd>

## Mixed Content Stress Test

> ### Quoted heading
>
> - [x] Task inside quote
> - [ ] Another task inside quote
>
> | Name | Status | Score |
> | --- | ---: | :---: |
> | Alpha | done | 10 |
> | Beta | pending | 7 |
>
> ```bash
> echo "quoted code block"
> ```

1. Ordered item with a paragraph.

   Additional paragraph text inside the same list item.

2. Ordered item with a nested unordered list:
   - child item one
   - child item two

3. Ordered item with a nested blockquote:

   > Nested quote inside a list item

## End

If all of the above renders correctly, the current `markdown_render` styling is covering the main Markdown and GFM cases we expect to support.
