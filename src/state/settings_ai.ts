import AtuinEnv from "@/atuin_env";
import { useSettingsState } from "@/components/Settings/Settings";
import { ModelSelection } from "@/rs-bindings/ModelSelection";
import { Settings } from "@/state/settings";

export interface OllamaSettings {
  enabled: boolean;
  endpoint: string;
  model: string;
}

export function useAIProviderSettings<T extends Record<string, any>>(provider: string, defaultValue: T): [T, (settings: T) => void, boolean] {
  const [settings, setSettings, isLoading] = useSettingsState(
    `ai.provider.${provider}.settings`,
    defaultValue as T,
    () => Settings.aiProviderSettings(provider),
    (settings: T) => Settings.aiProviderSettings(provider, settings),
  );
  return [settings, setSettings, isLoading];
};

export async function getAIProviderSettings<T extends Record<string, any>>(provider: string): Promise<T> {
  const value = await Settings.aiProviderSettings(provider);
  return value as T;
}

export async function getModelSelection(provider: string): Promise<Result<ModelSelection, string>> {
  if (provider === "atuinhub") {
    return Ok({
      type: "atuinHub",
      data: {
        model: "claude-opus-4-5-20251101",
        uri: AtuinEnv.url("/api/ai/proxy/"),
      }
    }) as Result<ModelSelection, string>
  } else if (provider === "ollama") {
    const settings = await getAIProviderSettings<OllamaSettings>("ollama");
    if (!settings.enabled) {
      return Err("Ollama is not enabled in settings");
    }
    if (!settings.model) {
      return Err("Ollama model is not set in settings");
    }

    return Ok({
      type: "ollama",
      data: {
        model: settings.model,
        uri: joinUrlParts([settings.endpoint, "v1/"]),
      }
    }) as Result<ModelSelection, string>
  } else {
    return Ok({
      type: "atuinHub",
      data: {
        model: "claude-opus-4-5-20251101",
        uri: AtuinEnv.url("/api/ai/proxy/"),
      }
    }) as Result<ModelSelection, string>
  }
}

function joinUrlParts(parts: string[]): string {
  return parts.map(p => p.replace(/\/+$/, '')).join('/').replace(/([^:]\/)\/+/g, '$1');
}
