# Dropdown

The dropdown block allows you to select from a list of options, with three different ways to source the options: fixed values, variables, or command output.

### Option Sources

The dropdown supports three different option sources:

1. **Fixed Options** - Manually defined static options
2. **Variable Options** - Options sourced from a template variable
3. **Command Output** - Options dynamically generated from shell command output

### Fixed Options

Add options manually as simple values or label:value pairs for more user-friendly displays.

```
Simple Option
User Friendly Name:horrible-uuid-value
```

When using label:value pairs, users see the friendly label but the underlying value is stored and used in templates.

### Variable Options

Reference a template variable containing newline or comma-separated values. The variable should contain the options list.

```handlebars
{{var.environment_list}}
```

### Command Output

Execute a shell command that returns a list of options. Supports multiple interpreters (bash, python3, node) and can return label:value pairs.

```bash
kubectl get pods --no-headers | awk '{print $1}'
```

### Custom Delimiter

By default, the dropdown uses `:` to separate labels from values (e.g., `Label:value`). If your data contains colons (such as URLs or timestamps), you can customize the delimiter in the dropdown settings.

Open the dropdown configuration modal and set the **Label/Value Delimiter** field to any character or string:

| Delimiter | Example |
|-----------|---------|
| `:` (default) | `Production:prod` |
| `\|` | `Production\|https://api.example.com:8080` |
| `::` | `My Label::my-value` |
| `->` | `Display Name->actual_value` |

This allows you to use label:value pairs even when your values contain the default delimiter character.

### Template Usage

The selected value can be accessed in other blocks using the variable name:

```handlebars
{{var.dropdown_name}}
```

All input fields are first rendered by the [templating](../../templating.md "mention") system, allowing for flexible configuration.
