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
2. Output cannot be captured. As we are emulating a terminal, the output contains a multitude of control codes, prompt output, etc. If you'd like to work with the output of a shell command, we recommend using a [script](script.md "mention") block.
3. Terminals are slower than scripts. Because we're spinning up a new pseudo-terminal in the background, startup time and resource usage will be a bit higher than a script
4. Interactive input can sometimes feel unnatural, and the scripting process is different than writing a bash script
