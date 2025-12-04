# Terminal

<figure class="img-light">
  <picture>
    <img src="../../../images/terminal-light.png" alt="Terminal block">
  </picture>
  <figcaption></figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../../images/terminal-dark.png" alt="Terminal block">
  </picture>
  <figcaption></figcaption>
</figure>

The Terminal block provides a fully functional, **interactive** shell directly within your Runbook. It behaves just like your local terminal, respecting your system configuration, environment variables, and shell preferences.

To execute a command, simply type it into the code input field and hit the play button.&#x20;

The terminal processes your input just as if you had typed it directly into your shell. Everything runs in the background, so you can navigate away, continue editing your Runbook, or collaborate with others while your command executes.

Everything within the input box is fed as input for the terminal, so interactive sessions can be nested as deep as you'd like. There is much more flexibility here than a normal bash script, with the following caveats

1. We cannot detect when a command within a terminal ends - we hope to have this resolved soon, but there's an additional layer of complexity here
2. Standard output cannot be captured directly. As we are emulating a terminal, the output contains a multitude of control codes, prompt output, etc. However, you can set template variables using `$ATUIN_OUTPUT_VARS` (see Variables section below). If you need to work with stdout directly, we recommend using a [script](script.md "mention") block.
3. Terminals are slower than scripts. Because we're spinning up a new pseudo-terminal in the background, startup time and resource usage will be a bit higher than a script
4. Interactive input can sometimes feel unnatural, and the scripting process is different than writing a bash script

## Variables

While terminal blocks don't support capturing stdout directly, they can set template variables when the terminal session exits. This is done by writing to the `$ATUIN_OUTPUT_VARS` file.

**Usage:**

```bash
# Simple format for single-line values
echo "name=value" >> $ATUIN_OUTPUT_VARS
echo "another_var=another_value" >> $ATUIN_OUTPUT_VARS

# Heredoc format for multiline values
echo "notes<<EOF" >> $ATUIN_OUTPUT_VARS
echo "This is line 1" >> $ATUIN_OUTPUT_VARS
echo "This is line 2" >> $ATUIN_OUTPUT_VARS
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
- **Timing**: Variables are captured when the terminal exits
- **Location**: Works with both local and remote (SSH) terminal sessions

**Example:**

```bash
# Set variables during an interactive session
echo "session_id=$(uuidgen)" >> $ATUIN_OUTPUT_VARS
echo "current_dir=$(pwd)" >> $ATUIN_OUTPUT_VARS

# Set a multiline variable with command output
echo "file_list<<END" >> $ATUIN_OUTPUT_VARS
ls -la >> $ATUIN_OUTPUT_VARS
echo "END" >> $ATUIN_OUTPUT_VARS

# Continue with other terminal commands
ls -la
```

These variables can then be referenced in other blocks:

```handlebars
{{var.session_id}}
{{var.current_dir}}
```

See the [templating](../../templating.md) section for full information on template variables.
