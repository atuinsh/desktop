import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

export class LocalDirectoryBlock extends Block {
  path: string;

  get typeName() {
    return "local-directory";
  }

  constructor(id: string, name: string, dependency: DependencySpec, path: string) {
    super(id, name, dependency);
    this.path = path;
  }

  serialize() {
    return JSON.stringify({
      id: this.id,
      name: this.name,
      path: this.path,
    });
  }

  static deserialize(json: string) {
    const data = JSON.parse(json);
    return new LocalDirectoryBlock(data.id, data.name, data.dependency, data.path);
  }

  object() {
    return {
      id: this.id,
      name: this.name,
      path: this.path,
    };
  }
}

export const LOCALDIRECTORY_BLOCK_SCHEMA = {
  type: "local-directory",
  propSchema: {
    // No path prop - stored in KV store instead
  },
  content: "none",
} as const;

AIBlockRegistry.getInstance().addBlock({
  typeName: "local-directory",
  friendlyName: "Local Directory",
  shortDescription:
    "Sets the current working directory for the runbook (local to the user's machine).",
  description: undent`
    Local Directory blocks set the current working directory for terminal and script blocks that follow. The path is stored locally on the user's machine and is not synced with the runbook,
    allowing different users to set different working directories for the same runbook.

    This block has no configurable props - users select the directory through a folder picker dialog.

    When a Local Directory block is present, subsequent terminal and script blocks will execute in that directory context.

    Note: Since the path is stored locally, you cannot set the path programmatically. This block is primarily used by users to configure their local environment.
    When generating a Local Directory block, you can instruct the user to select the directory through a folder picker dialog.

    Example: {
      "type": "local-directory",
      "props": {}
    }
  `,
});
