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

### Template Usage

The selected value can be accessed in other blocks using the variable name:

```handlebars
{{var.dropdown_name}}
```

All input fields are first rendered by the [templating](../../templating.md "mention") system, allowing for flexible configuration.
