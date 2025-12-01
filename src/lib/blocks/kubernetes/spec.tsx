import { KUBERNETES_BLOCK_SCHEMA, KubernetesBlock } from "@/lib/blocks/kubernetes";
import { DependencySpec } from "@/lib/workflow/dependency";
import { createReactBlockSpec } from "@blocknote/react";
import { KubernetesComponent } from "./component";
import track_event from "@/tracking";
import { Container } from "lucide-react";
import { Settings } from "@/state/settings";

export default createReactBlockSpec(
    KUBERNETES_BLOCK_SCHEMA,
    {
      // @ts-ignore
      render: ({ block, editor }) => {
        const handleCodeMirrorFocus = () => {
          // Ensure BlockNote knows which block contains the focused CodeMirror
          editor.setTextCursorPosition(block.id, "start");
        };

        const setName = (name: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, name: name },
          });
        };

        const setCommand = (command: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, command: command },
          });
        };

        const setMode = (mode: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, mode: mode },
          });
        };

        const setInterpreter = (interpreter: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, interpreter: interpreter },
          });
        };

        const setAutoRefresh = (autoRefresh: boolean) => {
          console.log("setting auto refresh in spec", autoRefresh);
          editor.updateBlock(block, {
            props: { ...block.props, autoRefresh: autoRefresh },
          });
        };

        const setRefreshInterval = (refreshInterval: number) => {
          console.log("setting refresh interval in spec", refreshInterval);
          editor.updateBlock(block, {
            props: { ...block.props, refreshInterval: refreshInterval },
          });
        };

        const setNamespace = (namespace: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, namespace: namespace },
          });
        };

        const setContext = (context: string) => {
          editor.updateBlock(block, {
            props: { ...block.props, context: context },
          });
        };



        let dependency = DependencySpec.deserialize(block.props.dependency || "{}");
        let kubernetes = new KubernetesBlock(
          block.id,
          block.props.name,
          dependency,
          block.props.command,
          block.props.mode as "preset" | "custom",
          block.props.interpreter,
          block.props.autoRefresh,
          block.props.refreshInterval,
          block.props.namespace || "",
          block.props.context || "",
        );

        return (
          <KubernetesComponent
            kubernetes={kubernetes}
            setName={setName}
            setCommand={setCommand}
            setMode={setMode}
            setInterpreter={setInterpreter}
            setAutoRefresh={setAutoRefresh}
            setRefreshInterval={setRefreshInterval}
            setNamespace={setNamespace}
            setContext={setContext}
            isEditable={editor.isEditable}
            onCodeMirrorFocus={handleCodeMirrorFocus}
          />
        );
      },
      toExternalHTML: ({ block }) => {
        return (
          <div>
            <h3>Kubernetes Get {block?.props?.mode === "preset" ? "Command" : "Custom Command"}</h3>
            <pre><code>{block?.props?.command}</code></pre>
          </div>
        );
      },
    },
  );

export const insertKubernetes = (editor: any) => ({
  title: "Kubernetes Get",
  subtext: "Execute kubectl get commands with live results",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "kubernetes-get" });

    // Count the number of kubernetes-get blocks
    let kubernetesBlocks = editor.document.filter((block: any) => block.type === "kubernetes-get");
    let name = `Kubernetes Get ${kubernetesBlocks.length + 1}`;

    Settings.getEffectiveScriptShell().then((interpreter) => {
      editor.insertBlocks(
        [
          {
            type: "kubernetes-get",
            props: {
              name,
              command: "kubectl get pods -o json",
              mode: "preset",
              interpreter,
            },
          },
        ],
        editor.getTextCursorPosition().block.id,
        "before",
      );
    });
  },
  icon: <Container size={18} />,
  aliases: ["kubernetes", "kubernetes-get", "k8s", "kubectl", "pods", "get"],
  group: "Execute",
});
