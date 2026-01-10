import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

export class TerminalBlock extends Block {
  code: string;
  outputVisible: boolean;

  get typeName() {
    return "terminal";
  }

  constructor(
    id: string,
    name: string,
    dependency: DependencySpec,
    code: string,
    outputVisible: boolean,
  ) {
    super(id, name, dependency);

    this.code = code;
    this.outputVisible = outputVisible;
  }

  serialize() {
    return JSON.stringify({
      id: this.id,
      name: this.name,
      code: this.code,
      outputVisible: this.outputVisible,
    });
  }

  static deserialize(json: string) {
    const data = JSON.parse(json);
    return new TerminalBlock(data.id, data.name, data.dependency, data.code, data.outputVisible);
  }

  object() {
    return {
      id: this.id,
      name: this.name,
      code: this.code,
      outputVisible: this.outputVisible,
    };
  }
}

export const DEFAULT_TERMINAL_ROWS = 20;

export const TERMINAL_BLOCK_SCHEMA = {
  type: "run",
  propSchema: {
    type: {
      default: "bash",
    },
    name: { default: "" },
    code: { default: "" },
    pty: { default: "" },
    global: { default: false },
    outputVisible: {
      default: true,
    },
    dependency: { default: "{}" },
    terminalRows: { default: DEFAULT_TERMINAL_ROWS },
  },
  content: "none",
} as const;

AIBlockRegistry.getInstance().addBlock({
  typeName: "run",
  friendlyName: "Terminal",
  shortDescription: "Executes commands in an interactive terminal.",
  description: undent`
    Terminal blocks are used to execute commands in an interactive terminal. This differs from a script block in that it allows for interactive input and output, and can be used to execute commands that require user input.
    Because of this, however, terminal blocks are slower, cannot have their output captured as a variable, and require an explicit 'exit' command for serial execution to continue. Since they execute
    in an interactive session, they are more flexible than script blocks, and since they load the user's environment, they can access the user's shell configuration and environment variables, which script blocks cannot.

    Because the output of a terminal block cannot be captured as a variable, it has no 'outputVariable' prop. If the script needs to write data to template variables, it can use the $ATUIN_OUTPUT_VARS file.

    The available props are:
    - name (string): The display name of the block
    - code (string): The command or commands to execute
    - type: (string): The type of terminal to use; Atuin Desktop supports bash, zsh, fish, python3, node, and sh, in addition to any user-defined shells.
    - outputVisible (boolean): Whether the output of the terminal should be visible to the user. Defaults to true.

    NOTE that Terminal blocks use 'type' instead of 'interpreter' to specify the shell interpreter to use.

    When using the Terminal block, you can reference template variables in code: {{ var.variable_name }}. You can escape variables with the 'shellquote' filter.

    OUTPUT ACCESS (requires block to have a name):
    - output.output (string): Terminal output (may be truncated for large outputs)
    - output.byte_count (number): Total bytes written to terminal
    - output.cancelled (boolean): Whether the block was cancelled

    For programmatic output capture, prefer Script blocks or use $ATUIN_OUTPUT_VARS.

    If the document has other script or terminal blocks using a specific shell type, default to using that shell type. Otherwise, you can ask the user, or use their default shell (accessible via the 'get_default_shell' tool).

    Example: {
        "type": "run",
        "props": {
          "name": "Say hello",
          "type": "bash",
          "code": "echo 'Hello, world!'",
        }
    }
  `,
});
