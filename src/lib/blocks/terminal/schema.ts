import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";

export class TerminalBlock extends Block {
    code: string;
    outputVisible: boolean;

    get typeName() {
        return "terminal";
    }

    constructor(id: string, name: string, dependency: DependencySpec, code: string, outputVisible: boolean) {
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

export const TERMINAL_LLM_PROMPT = `
    For 'run' blocks (terminal commands):
    - Focus on the 'code' property which contains the command
    - Can reference template variables: {{ var.variable_name }}
    - Common requests: fix syntax, optimize performance, add error handling, use template variables
    - Example: {"type": "run", "props": {"code": "curl {{ var.api_url }}/users", "name": "Get users", "outputVariable": "users_response"}, "id": "original-id"},
`;

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
  },
  content: "none",
} as const;
