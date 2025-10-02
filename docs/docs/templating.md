---
description: Create dynamic, reusable runbooks with MiniJinja templating system.
---

# Templating

#### **Templates**

Atuin uses [MiniJinja](https://docs.rs/minijinja) for rendering templates, enabling flexible output customization.

**Basic Syntax**

- **Variables**: `{{ var.variable_name }}`
  - Variables can be set by [script.md](blocks/executable/script.md "mention") blocks
- **Filters**: `{{ text | upper }}`, `{{ list | join(", ") }}`
- **Conditionals**:

```django
{% if var.foo %}
echo "foo is true"
{% else %}
echo "foo is false"
{% endif %}
```

- **Loops**:

```django
{% for remote in ["192.168.1.1", "192.168.1.2"] %}
echo "{{ remote }}"
{% endfor %}
```

**Built-in Functions**

- `range(n)`: Generates a sequence → `{% for i in range(3) %}{{ i }}{% endfor %}`
- `length(list)`: Gets list length → `{{ length(users) }}`
- `default(value, fallback)`: Uses fallback if `None` → `{{ user.name | default("Guest") }}`

**Custom Filters**

- `shellquote`: Escapes a string for safe use in shell commands → `{{ var.text | shellquote }}`
  - Uses POSIX single-quote escaping to handle special characters like quotes, backticks, dollar signs, etc.
  - Example: `echo {{ var.message | shellquote }}` safely handles any characters in the message variable
  - Particularly useful when passing variables that might contain user input or special characters

**Example Usage**

Before (verbose manual escaping):
```bash
echo "{{ var.test | replace("\"", "\\\"") }}"
```

After (using shellquote filter):
```bash
echo {{ var.test | shellquote }}
```

The `shellquote` filter handles all special shell characters automatically, including:
- Single quotes (`'`)
- Double quotes (`"`)
- Backticks (`` ` ``)
- Dollar signs (`$`)
- And other shell metacharacters

### Document access

The template system has full access to the entire document - blocks, text, etc.

!!! warning
    We are still iterating on this API, and it is likely to change in future releases

First, give a block a name. Click the pencil icon next to the default name in the top left.

Then, it can be referred to via the `{{ doc.named }}`map, within the template system

<figure class="img-light">
  <picture>
    <img src="../../images/templating-light.png" alt="Collaborations">
  </picture>
  <figcaption>An example using an editor block's content from another block, via the template system</figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../images/templating-dark.png" alt="Collaborations">
  </picture>
  <figcaption>An example using an editor block's content from another block, via the template system</figcaption>
</figure>
