// Preset kubectl commands for easy mode
export const PRESET_COMMANDS = {
  pods: "kubectl get pods -o json",
  services: "kubectl get services -o json", 
  deployments: "kubectl get deployments -o json",
  configmaps: "kubectl get configmaps -o json",
  secrets: "kubectl get secrets -o json",
  nodes: "kubectl get nodes -o json",
  namespaces: "kubectl get namespaces -o json",
} as const;

export type PresetCommand = keyof typeof PRESET_COMMANDS;
