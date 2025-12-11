// This technically does nothing with state right now, but given the future runbook state I'm working on
// it makes sense for it to go in here

import { invoke } from "@tauri-apps/api/core";
import RunbookBus from "@/lib/app/runbook_bus";

// /// Expects the string to template, and the current Blocknote document
// export async function templateString(
//   id: string,
//   input: string,
//   doc: any[],
//   runbook: string | null,
//   workspaceRoot?: string | null,
// ): Promise<string> {
//   let normalized = normalizeInput(input);

//   try {
//     let templated: string = await invoke("template_str", {
//       source: normalized,
//       blockId: id,
//       doc,
//       runbook,
//       workspaceRoot: workspaceRoot || null,
//     });

//     return templated;
//   } catch (error) {
//     // Show error toast for template parsing errors
//     addToast({
//       title: "Template Error",
//       description: `${error}`,
//       color: "danger",
//     });

//     return normalized; // Return original input when template fails
//   }
// }

export async function setTemplateVar(runbookId: string, name: string, value: string) {
  const changed = await invoke("set_template_var", {
    runbook: runbookId,
    name,
    value,
  });

  if (changed) {
    const bus = RunbookBus.get(runbookId);
    bus.emitVariableChanged(name, value);
  }

  return changed;
}

export async function getTemplateVar(runbookId: string, name: string) {
  return invoke<string>("get_template_var", {
    runbook: runbookId,
    name,
  });
}
