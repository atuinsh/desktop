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

See the [templating](../../templating.md) section for full information on template variables.
