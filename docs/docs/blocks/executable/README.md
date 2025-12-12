---
description: Run scripts, terminal commands, and manage environments with serial execution support.
---

# :material-play: Executable Blocks

Runbooks support a number of executable blocks. These blocks allow you to run scripts on your local machine, and are the building blocks of most workflows.

While the execution context + state of a script or terminal block is isolated and independent, state is pushed down from the runbook. For instance, using a [directory](directory.md) block, you can set the directory that all subsequent blocks should execute within.

## Serial Execution

Runbooks also support "serial execution". Click the play button in the top right, and runbooks will execute each block in sequence automatically.

<figure class="img-light">
  <picture>
    <img src="../../images/serial-light.png" alt="Serial execution">
  </picture>
  <figcaption></figcaption>
</figure>
<figure class="img-dark">
  <picture>
    <img src="../../images/serial-dark.png" alt="Serial execution">
  </picture>
  <figcaption></figcaption>
</figure>


!!! warning "Terminal Block Completion"
    There is one small caveat for terminal blocks - they must exit. We cannot automatically determine if a terminal block has completed. You must either click the stop button yourself, or include "exit" somewhere in your input.

## Available Executable Blocks

<div class="grid cards" markdown>

-   :material-folder:{ .lg .middle } **Directory**

    ---

    Set the working directory for subsequent blocks in your runbook.

    [:octicons-arrow-right-24: Learn more](directory.md)

-   :material-menu-down:{ .lg .middle } **Dropdown**

    ---

    Create interactive dropdown selections for dynamic runbook execution.

    [:octicons-arrow-right-24: Learn more](dropdown.md)

-   :material-variable:{ .lg .middle } **Environment**

    ---

    Set environment variables for use in other blocks.

    [:octicons-arrow-right-24: Learn more](env.md)

-   :simple-kubernetes:{ .lg .middle } **Kubernetes**

    ---

    Execute kubectl commands and manage Kubernetes resources.

    [:octicons-arrow-right-24: Learn more](kubernetes.md)

-   :material-script:{ .lg .middle } **Script**

    ---

    Run custom scripts in various languages (bash, python, etc.).

    [:octicons-arrow-right-24: Learn more](script.md)

-   :material-console:{ .lg .middle } **Terminal**

    ---

    Interactive terminal sessions for complex command sequences.

    [:octicons-arrow-right-24: Learn more](terminal.md)

-   :material-tag:{ .lg .middle } **Variable**

    ---

    Define and manage template variables throughout your runbook.

    [:octicons-arrow-right-24: Learn more](variable.md)

-   :material-pause:{ .lg .middle } **Pause**

    ---

    Halt workflow execution for manual intervention or approval steps.

    [:octicons-arrow-right-24: Learn more](pause.md)

</div>
