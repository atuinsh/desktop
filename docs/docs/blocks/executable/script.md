# :material-script: Script

<figure class="img-light">
  <picture>
    <img src="../../../images/script-basic-light.png" alt="Script block">
  </picture>
  <figcaption></figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/script-basic-dark.png" alt="Script block">
  </picture>
  <figcaption></figcaption>
</figure>

The script block is a lot like a terminal block, except non-interactive. This is essentially your normal bash script, with some superpowers. Script blocks run without user interaction, making them perfect for automation and batch processing.

## Supported Interpreters

We support several different interpreters with the script block. While we default to `zsh`, we also support running code with the following:

1. **bash** - Standard Unix shell scripting
2. **python3** - Python scripts for data processing and automation  
3. **node** - JavaScript/Node.js for web API interactions

The current in-use interpreter can be changed via the dropdown in the top right of the block.

## Variables

Script blocks support two methods for setting template variables:

### Output Variable Capture

The output of a script block can be captured as a variable, and reused as input for other blocks. All input fields in all blocks are templated.

Set the "output variable" name in the header of the block. You can refer to the variable with the following syntax:

```handlebars
{{var.variable_name}}
```

<figure class="img-light">
  <picture>
    <img src="../../../images/script-light.png" alt="Script block with variables">
  </picture>
  <figcaption></figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/script-dark.png" alt="Script block with variables">
  </picture>
  <figcaption></figcaption>
</figure>

### Setting Variables via $ATUIN_OUTPUT_VARS

For more flexibility, scripts can set multiple variables by writing to the `$ATUIN_OUTPUT_VARS` file. This approach mirrors GitHub Actions' output variable syntax and allows a single script to set multiple template variables.

**Usage:**

```bash
# Simple format for single-line values
echo "name=value" >> $ATUIN_OUTPUT_VARS
echo "another_var=another_value" >> $ATUIN_OUTPUT_VARS

# Heredoc format for multiline values
echo "config<<EOF" >> $ATUIN_OUTPUT_VARS
echo "Line 1" >> $ATUIN_OUTPUT_VARS
echo "Line 2" >> $ATUIN_OUTPUT_VARS
echo "EOF" >> $ATUIN_OUTPUT_VARS

# For longer multiline strings, use a command group for efficiency
{
  echo "myvar<<EOF"
  echo "Some"
  echo "Multiline"
  echo "String"
  echo "EOF"
} >> $ATUIN_OUTPUT_VARS
```

- **Format**: Two formats supported:
  - Simple: `KEY=VALUE` entries, one per line
  - Heredoc: `KEY<<DELIMITER` followed by content lines until `DELIMITER` (for multiline values)
- **Timing**: Variables are captured when the script exits successfully (exit code 0)
- **Location**: Works with both local and remote (SSH) script execution

**Example:**

```bash
# Generate multiple outputs from a single script
echo "timestamp=$(date +%s)" >> $ATUIN_OUTPUT_VARS
echo "hostname=$(hostname)" >> $ATUIN_OUTPUT_VARS
echo "user=$(whoami)" >> $ATUIN_OUTPUT_VARS

# Capture multiline command output using heredoc
echo "disk_usage<<END" >> $ATUIN_OUTPUT_VARS
df -h >> $ATUIN_OUTPUT_VARS
echo "END" >> $ATUIN_OUTPUT_VARS

# Or build multiline content programmatically
echo "report<<REPORT" >> $ATUIN_OUTPUT_VARS
echo "Server: $(hostname)" >> $ATUIN_OUTPUT_VARS
echo "Status: Running" >> $ATUIN_OUTPUT_VARS
echo "Uptime: $(uptime)" >> $ATUIN_OUTPUT_VARS
echo "REPORT" >> $ATUIN_OUTPUT_VARS
```

These variables can then be referenced in other blocks:

```handlebars
{{var.timestamp}}
{{var.hostname}}
{{var.user}}
```

See the [templating](../../templating.md) section for full information on template variables.

## Block Output

Script blocks produce structured output that can be accessed in templates after execution. See [Block Output](../index.md#block-output) for general information on accessing block output.

### Accessing Script Results

```jinja
{%- set output = doc.named['my_script'].output %}

Exit code: {{ output.exit_code }}
Output: {{ output.stdout }}
```

### Output Fields

| Field | Type | Description |
|-------|------|-------------|
| `exit_code` | number | Script exit code (0 for success, non-zero for failure) |
| `stdout` | string | All stdout output combined as a single string |
| `stderr` | string | All stderr output combined as a single string |
| `combined` | string | Both stdout and stderr combined in execution order |

### Example Usage

```jinja
{%- set output = doc.named['check_script'].output %}

{% if output.exit_code == 0 %}
  Script succeeded!
  {{ output.stdout }}
{% else %}
  Script failed with exit code {{ output.exit_code }}
  Error: {{ output.stderr }}
{% endif %}
```

### Working with Output

```jinja
{%- set output = doc.named['system_info'].output %}

{# Access just stdout #}
System info: {{ output.stdout }}

{# Access just stderr (errors/warnings) #}
{% if output.stderr %}
  Warnings: {{ output.stderr }}
{% endif %}

{# Or get everything combined #}
Full output: {{ output.combined }}
```

### Combining with Output Variables

Script block output complements the `$ATUIN_OUTPUT_VARS` mechanism. Use output variables for specific data extraction and block output for checking execution status:

```jinja
{%- set output = doc.named['deploy_script'].output %}

{% if output.exit_code == 0 %}
  Deployment succeeded!
  Version: {{ var.deployed_version }}
  Time: {{ var.deploy_time }}
{% else %}
  Deployment failed: {{ output.stderr }}
{% endif %}
```
