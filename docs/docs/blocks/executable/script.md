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
echo "name=value" >> $ATUIN_OUTPUT_VARS
echo "another_var=another_value" >> $ATUIN_OUTPUT_VARS
```

- Format: `KEY=VALUE` entries, one per line
- Variables are captured when the script exits successfully (exit code 0)
- Works with both local and remote (SSH) script execution

**Example:**

```bash
# Generate multiple outputs from a single script
echo "timestamp=$(date +%s)" >> $ATUIN_OUTPUT_VARS
echo "hostname=$(hostname)" >> $ATUIN_OUTPUT_VARS
echo "user=$(whoami)" >> $ATUIN_OUTPUT_VARS
```

These variables can then be referenced in other blocks:

```handlebars
{{var.timestamp}}
{{var.hostname}}
{{var.user}}
```

See the [templating](../../templating.md) section for full information on template variables.
