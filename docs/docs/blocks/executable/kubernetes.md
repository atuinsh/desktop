# Kubernetes

The Kubernetes block allows you to execute kubectl get commands and display the results in a formatted table directly within your runbook.

### Operating Modes

The Kubernetes block supports two different modes:

1. **Preset Mode** - Select from common kubectl get commands
2. **Custom Mode** - Execute custom kubectl get commands

### Preset Commands

Preset mode provides quick access to common kubectl get operations:

- **Pods** - List all pods in the namespace
- **Services** - List all services
- **Deployments** - List all deployments
- **ConfigMaps** - List all config maps
- **Secrets** - List all secrets
- **Nodes** - List all cluster nodes
- **Namespaces** - List all namespaces

### Custom Commands

Custom mode allows you to execute kubectl get commands with custom parameters. Commands must return JSON output for proper parsing and table display.

```bash
kubectl get pods -l app=nginx -o json
```

### Context and Namespace

Configure the Kubernetes context and namespace for your commands:

- **Context** - Target Kubernetes cluster context
- **Namespace** - Target namespace (if applicable)

### Auto-refresh

Enable automatic command execution at configurable intervals to keep data current. Useful for monitoring resources that change frequently.

### Template Usage

All input fields support [templating](../../templating.md "mention"), allowing you to use variables in your commands:

```bash
kubectl get pods -n {{var.target_namespace}}
```

## Block Output

Kubernetes blocks produce structured output that can be accessed in templates after execution. See [Block Output](../index.md#block-output) for general information on accessing block output.

### Accessing Resource Data

```jinja
{%- set output = doc.named['my_k8s_query'].output %}

{# Access table data #}
{% for row in output.data %}
  {{ row[0] }}  {# First column value #}
{% endfor %}
```

### Output Fields

| Field | Type | Description |
|-------|------|-------------|
| `data` | array | Table rows (each row is an array of cell values) |
| `columns` | array | Column definitions with `id`, `title`, and `width` |
| `first_row` | array | The first row of data (convenience accessor) |
| `item_count` | number | Number of resources returned |
| `resource_kind` | string | Detected resource type (e.g., "pod", "service", "deployment") |
| `raw_output` | string | Raw command output if not parsed as JSON |
| `stderr` | string | Standard error output if any |
| `has_table` | boolean | Whether structured table data is available |

### Column Structure

Each column in the `columns` array contains:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Column identifier |
| `title` | string | Display title |
| `width` | number | Suggested column width |

### Example Usage

```jinja
{%- set output = doc.named['pod_list'].output %}

{% if output.has_table %}
  Found {{ output.item_count }} {{ output.resource_kind }}s:

  {% for row in output.data %}
  - Name: {{ row[0] }}, Namespace: {{ row[1] }}, Status: {{ row[3] }}
  {% endfor %}
{% else %}
  Raw output: {{ output.raw_output }}
{% endif %}
```

### Future Enhancements

Currently, the Kubernetes block focuses on `kubectl get` operations for resource inspection and monitoring. We plan to expand Kubernetes functionality in future releases to include:

- Resource creation and modification operations
- Advanced kubectl commands (apply, delete, patch, etc.)
- Helm chart management
- Additional output formats beyond JSON
