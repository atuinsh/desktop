import { DependencySpec } from "../../workflow/dependency";
import Block from "../../workflow/blocks/block";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

export class KubernetesBlock extends Block {
  command: string;
  mode: "preset" | "custom";
  interpreter: string;
  autoRefresh: boolean;
  refreshInterval: number;
  namespace: string;
  context: string;

  get typeName() {
    return "kubernetes-get";
  }

  constructor(
    id: string,
    name: string,
    dependency: DependencySpec,
    command: string,
    mode: "preset" | "custom",
    interpreter: string,
    autoRefresh: boolean,
    refreshInterval: number,
    namespace: string,
    context: string,
  ) {
    super(id, name, dependency);

    this.command = command;
    this.mode = mode;
    this.interpreter = interpreter;
    this.autoRefresh = autoRefresh;
    this.refreshInterval = refreshInterval;
    this.namespace = namespace;
    this.context = context;
  }

  serialize() {
    return JSON.stringify({
      id: this.id,
      name: this.name,
      command: this.command,
      mode: this.mode,
      interpreter: this.interpreter,
      autoRefresh: this.autoRefresh,
      refreshInterval: this.refreshInterval,
      namespace: this.namespace,
      context: this.context,
    });
  }

  static deserialize(json: string) {
    const data = JSON.parse(json);
    return new KubernetesBlock(
      data.id,
      data.name,
      data.dependency,
      data.command,
      data.mode,
      data.interpreter,
      data.autoRefresh,
      data.refreshInterval,
      data.namespace,
      data.context,
    );
  }

  object() {
    return {
      id: this.id,
      name: this.name,
      command: this.command,
      mode: this.mode,
      interpreter: this.interpreter,
      autoRefresh: this.autoRefresh,
      refreshInterval: this.refreshInterval,
      namespace: this.namespace,
      context: this.context,
    };
  }
}

export const KUBERNETES_BLOCK_SCHEMA = {
  type: "kubernetes-get",
  propSchema: {
    name: { default: "" },
    command: { default: "kubectl get pods -o json" },
    mode: { default: "preset" },
    interpreter: { default: "bash" },
    autoRefresh: { default: false },
    refreshInterval: { default: 0 },
    namespace: { default: "" },
    context: { default: "" },
    dependency: { default: "{}" },
  },
  content: "none",
} as const;

AIBlockRegistry.getInstance().addBlock({
  typeName: "kubernetes-get",
  friendlyName: "Kubernetes Get",
  shortDescription: "Execute kubectl get commands with live results.",
  description: undent`
    Kubernetes Get blocks execute kubectl get commands and display the results in a rich, interactive format. They support two modes: 'preset' for common kubectl commands, or 'custom' for any kubectl command.

    The available props are:
    - name (string): The display name of the block
    - command (string): The full kubectl command (see presets below)
    - mode (string): Either 'preset' or 'custom'
    - interpreter (string): The shell to use for execution (bash, zsh, etc.)
    - autoRefresh (boolean): Whether to automatically refresh the results
    - refreshInterval (number): The interval in seconds between refreshes
    - namespace (string): The Kubernetes namespace to query
    - context (string): The Kubernetes context to use

    When using the Kubernetes Get block, you can reference template variables in the command: {{ var.variable_name }}.

    PRESET MODE:
    For preset mode, set 'command' to the full kubectl command:
    - pods: "kubectl get pods -o json"
    - services: "kubectl get services -o json"
    - deployments: "kubectl get deployments -o json"
    - configmaps: "kubectl get configmaps -o json"
    - secrets: "kubectl get secrets -o json"
    - nodes: "kubectl get nodes -o json"
    - namespaces: "kubectl get namespaces -o json"

    CUSTOM MODE:
    Include '-o json' for proper parsing and table display. Non-JSON output displays in a less rich format.

    OUTPUT ACCESS (requires block to have a name):
    - output.data (array): Parsed Kubernetes items
    - output.columns (array): Column definitions for display
    - output.item_count (number): Number of items returned
    - output.resource_kind (string): Type of resource queried

    Example: {
      "type": "kubernetes-get",
      "props": {
        "name": "Pod List",
        "command": "kubectl get pods -o json",
        "mode": "preset",
        "namespace": "{{ var.namespace }}"
      }
    }
  `,
});
