// This technically does nothing with state right now, but given the future runbook state I'm working on
// it makes sense for it to go in here

import { normalizeInput } from "@/lib/utils";
import { invoke } from "@tauri-apps/api/core";

import { addToast } from "@heroui/react";
import RunbookBus from "@/lib/app/runbook_bus";

export enum TemplateErrorBehavior {
  TOAST_ON_ERROR = "toast_on_error",
  SUPPRESS_ERROR = "suppress_error",
}

/// Expects the string to template, and the current Blocknote document
export async function templateString(
  id: string,
  input: string,
  doc: any[],
  runbook: string | null,
  errorBehavior: TemplateErrorBehavior = TemplateErrorBehavior.TOAST_ON_ERROR,
): Promise<string> {
  let normalized = normalizeInput(input);

  try {
    let templated: string = await invoke("template_str", {
      source: normalized,
      blockId: id,
      doc,
      runbook,
    });

    return templated;
  } catch (error) {
    if (errorBehavior === TemplateErrorBehavior.TOAST_ON_ERROR) {
      // Show error toast for template parsing errors
      addToast({
        title: "Template Error",
        description: `${error}`,
        color: "danger",
      });
    }

    return normalized; // Return original input when template fails
  }
}

export async function getDependentVariables(input: string): Promise<string[]> {
  let normalized = normalizeInput(input);

  try {
    let dependent: string[] = await invoke("get_dependent_variables", {
      source: normalized,
    });

    return dependent;
  } catch (error) {
    return [];
  }
}

export async function setTemplateVar(runbookId: string, name: string, value: string, source?: any) {
  const changed = await invoke("set_template_var", {
    runbook: runbookId,
    name,
    value,
  });

  if (changed) {
    const bus = RunbookBus.get(runbookId);
    bus.emitVariableChanged(name, value, source);
  }

  return changed;
}

export async function getTemplateVar(runbookId: string, name: string) {
  return invoke<string>("get_template_var", {
    runbook: runbookId,
    name,
  });
}
