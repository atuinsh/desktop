import { useState, useEffect } from "react";
import { isAppleDevice } from "@react-aria/utils";
import { open } from "@tauri-apps/plugin-shell";
import { PlusIcon, TrashIcon, PlayIcon } from "lucide-react";
import {
  Autocomplete,
  AutocompleteItem,
  Input,
  Switch,
  Card,
  CardBody,
  Spinner,
  cn,
  Select,
  SelectItem,
  SharedSelection,
  Button,
  User,
  Modal,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalContent,
  Link,
  useDisclosure,
  addToast,
  Slider,
  Tabs,
  Tab,
} from "@heroui/react";
import { Settings } from "@/state/settings";
import { KVStore } from "@/state/kv";
import { relaunch } from "@tauri-apps/plugin-process";
import { useStore } from "@/state/store";
import { invoke } from "@tauri-apps/api/core";
import { useAsyncData } from "@/lib/utils";
import SocketManager from "@/socket";
import handleDeepLink from "@/routes/root/deep";
import * as api from "@/api/api";
import InterpreterSelector from "@/lib/blocks/common/InterpreterSelector";
import AtuinEnv from "@/atuin_env";
import { OllamaSettings, useAIProviderSettings } from "@/state/settings_ai";

async function loadFonts(): Promise<string[]> {
  const fonts = await invoke<string[]>("list_fonts");
  fonts.push("Inter");
  fonts.push("FiraCode");
  fonts.sort();
  return fonts;
}

// Custom hook for managing settings
export const useSettingsState = (
  _key: any,
  initialValue: any,
  settingsGetter: any,
  settingsSetter: any,
) => {
  const [value, setValue] = useState(initialValue);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const loadSetting = async () => {
      const savedValue = await settingsGetter();
      setValue(savedValue || initialValue);
      setIsLoading(false);
    };
    loadSetting();
  }, [settingsGetter, initialValue]);

  const updateSetting = async (newValue: any) => {
    setValue(newValue);
    await settingsSetter(newValue);
  };

  return [value, updateSetting, isLoading];
};

interface SettingsInputProps {
  label: string;
  value: string;
  onChange: (e: string) => void;
  placeholder: string;
  description: string;
  type: string;
}

// Reusable setting components
const SettingInput = ({
  label,
  value,
  onChange,
  placeholder,
  description,
  type,
}: SettingsInputProps) => (
  <Input
    label={label}
    value={value}
    type={type}
    onChange={(e) => onChange(e.target.value)}
    placeholder={placeholder}
    description={description}
  />
);

interface SettingsSwitchProps {
  label: string;
  isSelected: boolean;
  onValueChange: (e: boolean) => void;
  description: string;
  className?: string;
  isDisabled?: boolean;
}

const SettingSwitch = ({
  label,
  isSelected,
  onValueChange,
  description,
  className,
  isDisabled,
}: SettingsSwitchProps) => (
  <Switch
    isSelected={isSelected}
    onValueChange={onValueChange}
    className={cn("flex justify-between items-center w-full", className)}
    isDisabled={isDisabled || false}
  >
    <div className="flex flex-col">
      <span>{label}</span>
      {description && <span className="text-tiny text-default-400">{description}</span>}
    </div>
  </Switch>
);

// Settings sections
const GeneralSettings = () => {
  const [showingPromptToRestart, setShowingPromptToRestart] = useState(false);

  function promptToRestart() {
    if (showingPromptToRestart) return;
    setShowingPromptToRestart(true);

    addToast({
      title: "Restart required",
      description: "Atuin needs to restart to apply your changes. This won't take long!",
      color: "primary",
      radius: "sm",
      timeout: Infinity,
      shouldShowTimeoutProgress: false,
      onClose: () => {
        setShowingPromptToRestart(false);
      },
      endContent: (
        <Button size="sm" variant="flat" color="primary" className="p-2" onPress={() => relaunch()}>
          Restart
        </Button>
      ),
    });
  }

  const fonts = useAsyncData(loadFonts, []);

  const [trackingOptIn, setTrackingOptIn, isLoading] = useSettingsState(
    "usage_tracking",
    false,
    async () => {
      const db = await KVStore.open_default();
      return await db.get("usage_tracking");
    },
    async (value: boolean) => {
      const db = await KVStore.open_default();
      await db.set("usage_tracking", value);
      promptToRestart();
    },
  );

  const colorMode = useStore((state) => state.colorMode);
  const fontSize = useStore((state) => state.fontSize);
  const fontFamily = useStore((state) => state.fontFamily);
  const sidebarClickStyle = useStore((state) => state.sidebarClickStyle);
  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const backgroundSync = useStore((state) => state.backgroundSync);
  const syncConcurrency = useStore((state) => state.syncConcurrency);
  const uiScale = useStore((state) => state.uiScale);
  const setUiScale = useStore((state) => state.setUiScale);
  const [localUiScale, setLocalUiScale] = useState(uiScale);

  useEffect(() => {
    setLocalUiScale(uiScale);
  }, [uiScale]);

  const [vimModeEnabled, setVimModeEnabledState, vimModeLoading] = useSettingsState(
    "editor_vim_mode",
    false,
    Settings.editorVimMode,
    Settings.editorVimMode,
  );

  function setVimModeEnabled(enabled: boolean) {
    setVimModeEnabledState(enabled);
    useStore.getState().setVimModeEnabled(enabled);
  }

  const [shellCheckEnabled, setShellCheckEnabledState, shellCheckEnabledLoading] = useSettingsState(
    "shellcheck_enabled",
    false,
    Settings.shellCheckEnabled,
    Settings.shellCheckEnabled,
  );

  const [shellCheckPath, setShellCheckPathState, shellCheckPathLoading] = useSettingsState(
    "shellcheck_path",
    "",
    Settings.shellCheckPath,
    Settings.shellCheckPath,
  );

  function setShellCheckEnabled(enabled: boolean) {
    setShellCheckEnabledState(enabled);
    useStore.getState().setShellCheckEnabled(enabled);
  }

  function setShellCheckPath(path: string) {
    setShellCheckPathState(path);
    useStore.getState().setShellCheckPath(path);
  }

  const themes = [
    ["Abcdef", "abcdef"],
    ["Abyss", "abyss"],
    ["Androidstudio", "androidstudio"],
    ["Andromeda", "andromeda"],
    ["Atomone", "atomone"],
    ["Aura", "aura"],
    ["Basic Light", "basicLight"],
    ["Basic Dark", "basicDark"],
    ["Bbedit", "bbedit"],
    ["Bespin", "bespin"],
    ["Console Dark", "consoleDark"],
    ["Console Light", "consoleLight"],
    ["Copilot", "copilot"],
    ["Darcula", "darcula"],
    ["Dracula", "dracula"],
    ["Duotone Light", "duotoneLight"],
    ["Duotone Dark", "duotoneDark"],
    ["Eclipse", "eclipse"],
    ["GitHub Light", "githubLight"],
    ["GitHub Dark", "githubDark"],
    ["Gruvbox Dark", "gruvboxDark"],
    ["Gruvbox Light", "gruvboxLight"],
    ["Kimbie", "kimbie"],
    ["Material Light", "materialLight"],
    ["Material Dark", "materialDark"],
    ["Monokai", "monokai"],
    ["Monokai Dimmed", "monokaiDimmed"],
    ["Noctis Lilac", "noctisLilac"],
    ["Nord", "nord"],
    ["Okaidia", "okaidia"],
    ["Red", "red"],
    ["Quietlight", "quietlight"],
    ["Solarized Light", "solarizedLight"],
    ["Solarized Dark", "solarizedDark"],
    ["Sublime", "sublime"],
    ["Tokyo Night", "tokyoNight"],
    ["Tokyo Night Storm", "tokyoNightStorm"],
    ["Tokyo Night Day", "tokyoNightDay"],
    ["Tomorrow Night Blue", "tomorrowNightBlue"],
    ["VS Code Dark", "vscodeDark"],
    ["VS Code Light", "vscodeLight"],
    ["White Light", "whiteLight"],
    ["White Dark", "whiteDark"],
    ["Xcode Light", "xcodeLight"],
    ["Xcode Dark", "xcodeDark"],
  ];

  function setColorMode(keys: SharedSelection) {
    useStore.getState().setColorMode(keys.currentKey as "light" | "dark" | "system");
  }

  function setSidebarClickStyle(keys: SharedSelection) {
    useStore.getState().setSidebarClickStyle(keys.currentKey as "link" | "explorer");
  }

  function setFontSize(fontSize: number) {
    useStore.getState().setFontSize(fontSize);
  }

  function setFontFamily(fontFamily: any) {
    useStore.getState().setFontFamily(fontFamily);
  }

  function setLightModeEditorTheme(keys: SharedSelection) {
    useStore.getState().setLightModeEditorTheme(keys.currentKey as string);
  }

  function setDarkModeEditorTheme(keys: SharedSelection) {
    useStore.getState().setDarkModeEditorTheme(keys.currentKey as string);
  }

  function setBackgroundSync(backgroundSync: boolean) {
    useStore.getState().setBackgroundSync(backgroundSync);
    promptToRestart();
  }

  function setSyncConcurrency(keys: SharedSelection) {
    const syncConcurrency = parseInt(keys.currentKey as string, 10);
    useStore.getState().setSyncConcurrency(syncConcurrency);
    promptToRestart();
  }

  if (isLoading || vimModeLoading || shellCheckEnabledLoading || shellCheckPathLoading)
    return <Spinner />;

  return (
    <>
      <Card shadow="sm" className="w-full">
        <CardBody>
          <h2 className="text-xl font-semibold">General</h2>

          <SettingSwitch
            className="mt-4"
            label="Enable usage tracking"
            isSelected={trackingOptIn}
            onValueChange={setTrackingOptIn}
            description="Track usage and errors to improve Atuin"
          />
          <Select
            label="Color Mode"
            value={colorMode}
            onSelectionChange={setColorMode}
            className="mt-8"
            placeholder="Select color mode"
            selectedKeys={[colorMode]}
          >
            <SelectItem key="light" textValue="Light">
              Light
            </SelectItem>
            <SelectItem key="dark" textValue="Dark">
              Dark
            </SelectItem>
            <SelectItem key="system" textValue="System">
              Follow System
            </SelectItem>
          </Select>

          <div className="mt-6">
            <div className="flex items-end gap-6">
              <Slider
                label="UI Scale"
                size="md"
                step={10}
                minValue={50}
                maxValue={150}
                value={localUiScale}
                onChange={(val: number | number[]) => {
                  const numVal = Array.isArray(val) ? val[0] : val;
                  setLocalUiScale(numVal);
                }}
                onChangeEnd={(val: number | number[]) => {
                  const numVal = Array.isArray(val) ? val[0] : val;
                  setUiScale(numVal);
                }}
                marks={[
                  { value: 50, label: "50%" },
                  { value: 100, label: "100%" },
                  { value: 150, label: "150%" },
                ]}
                hideValue
                className="flex-1"
              />
              <Input
                type="number"
                size="sm"
                min={50}
                max={150}
                step={10}
                value={localUiScale.toString()}
                onChange={(e) => {
                  const val = parseInt(e.target.value);
                  if (!isNaN(val)) {
                    setLocalUiScale(val);
                  }
                }}
                onBlur={() => {
                  const clampedVal = Math.min(150, Math.max(50, localUiScale));
                  setLocalUiScale(clampedVal);
                  setUiScale(clampedVal);
                }}
                endContent={<span className="text-default-400 text-small">%</span>}
                classNames={{
                  base: "w-24",
                  input: "text-right",
                }}
              />
            </div>
            <p className="text-tiny text-default-400 mt-1">
              Adjust the overall UI size. Use {isAppleDevice() ? "Cmd" : "Ctrl"}+/- to quickly zoom, {isAppleDevice() ? "Cmd" : "Ctrl"}+0 to reset.
            </p>
          </div>

          <div className="flex flex-row gap-4 mt-4">
            <Autocomplete
              label="Font"
              value={fontFamily}
              selectedKey={fontFamily}
              onSelectionChange={setFontFamily}
              description="Font to use for the Runbook editor"
              defaultItems={fonts?.map((font) => ({ label: font, key: font })) || []}
            >
              {(item) => <AutocompleteItem key={item.key}>{item.label}</AutocompleteItem>}
            </Autocomplete>

            <div>
              <Input
                label="Font Size"
                type="number"
                value={fontSize.toString()}
                onChange={(e) => setFontSize(parseInt(e.target.value))}
              />
            </div>
          </div>
          <Select
            label="Runbook selection style"
            value={sidebarClickStyle}
            onSelectionChange={setSidebarClickStyle}
            className="mt-2"
            placeholder="Select sidebar click style"
            selectedKeys={[sidebarClickStyle]}
          >
            <SelectItem key="link" textValue="Click to open">
              Click to open
            </SelectItem>
            <SelectItem key="explorer" textValue="Click to select, double click to open">
              Click to select, double click to open
            </SelectItem>
          </Select>

          <div className="mt-4 flex flex-row gap-4">
            <SettingSwitch
              label="Enable background sync"
              isSelected={backgroundSync}
              onValueChange={setBackgroundSync}
              description="Sync runbooks in the background"
            />
            <Select
              label="Number of runbooks to sync concurrently"
              value={syncConcurrency.toString()}
              onSelectionChange={setSyncConcurrency}
              className="mt-4"
              placeholder="Select sync concurrency"
              selectedKeys={[syncConcurrency.toString()]}
              disabled={!backgroundSync}
              items={[
                { label: "1 (no concurrency)", key: "1" },
                { label: "2", key: "2" },
                { label: "5", key: "5" },
                { label: "10", key: "10" },
              ]}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </div>
        </CardBody>
      </Card>

      <Card shadow="sm">
        <CardBody>
          <h2 className="text-xl font-semibold">Editor</h2>

          <Select
            label="Light mode editor theme"
            value={lightModeEditorTheme}
            onSelectionChange={setLightModeEditorTheme}
            className="mt-4"
            placeholder="Select light mode editor theme"
            selectedKeys={[lightModeEditorTheme]}
            items={themes.map((theme) => ({ label: theme[0], key: theme[1] }))}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>

          <Select
            label="Dark mode editor theme"
            value={darkModeEditorTheme}
            onSelectionChange={setDarkModeEditorTheme}
            className="mt-4"
            placeholder="Select dark mode editor theme"
            selectedKeys={[darkModeEditorTheme]}
            items={themes.map((theme) => ({ label: theme[0], key: theme[1] }))}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>

          <SettingSwitch
            className="mt-4"
            label="Enable Vim mode"
            isSelected={vimModeEnabled}
            onValueChange={setVimModeEnabled}
            description="Enable Vim key bindings in code editors"
          />

          <SettingSwitch
            className="mt-4"
            label="Enable ShellCheck"
            isSelected={shellCheckEnabled}
            onValueChange={setShellCheckEnabled}
            description="Enable ShellCheck static analysis for shell scripts in code editors"
          />

          {shellCheckEnabled && (
            <div className="mt-4">
              <SettingInput
                type="text"
                label="ShellCheck path"
                value={shellCheckPath || ""}
                onChange={setShellCheckPath}
                placeholder=""
                description="(Optional) Path to the ShellCheck command line tool if it's not allready in PATH"
              />
            </div>
          )}

          <div className="mt-2 ml-1">
            <Link
              isExternal
              href="https://uiwjs.github.io/react-codemirror/#/theme/home"
              className="text-sm text-blue-500 underline"
              onPress={() => open("https://uiwjs.github.io/react-codemirror/#/theme/home")}
            >
              Preview available themes
            </Link>
          </div>
        </CardBody>
      </Card>
    </>
  );
};

const RunbookSettings = () => {
  const fonts = useAsyncData(loadFonts, []);
  const [scriptInterpreters, setScriptInterpreters] = useState<
    Array<{ command: string; name: string }>
  >([]);
  const [newInterpreterName, setNewInterpreterName] = useState("");
  const [newInterpreterCommand, setNewInterpreterCommand] = useState("");
  const [systemDefaultShell, setSystemDefaultShell] = useState<string>("bash");

  // Load script interpreters and system default shell
  useEffect(() => {
    Settings.scriptInterpreters().then((interpreters) => {
      setScriptInterpreters(interpreters);
    });
    Settings.getSystemDefaultShell().then((shell) => {
      setSystemDefaultShell(shell);
    });
  }, []);

  // Save script interpreters
  const saveScriptInterpreters = (interpreters: Array<{ command: string; name: string }>) => {
    setScriptInterpreters(interpreters);
    Settings.setScriptInterpreters(interpreters);
  };

  const addScriptInterpreter = () => {
    if (!newInterpreterCommand || !newInterpreterName) return;

    const newInterpreters = [
      ...scriptInterpreters,
      {
        name: newInterpreterName,
        command: newInterpreterCommand,
      },
    ];

    saveScriptInterpreters(newInterpreters);
    setNewInterpreterCommand("");
    setNewInterpreterName("");
  };

  const removeScriptInterpreter = (command: string) => {
    const newInterpreters = scriptInterpreters.filter((i) => i.command !== command);
    saveScriptInterpreters(newInterpreters);
  };

  const [terminalFont, setTerminalFont, fontLoading] = useSettingsState(
    "terminal_font",
    "",
    Settings.terminalFont,
    Settings.terminalFont,
  );
  const [terminalFontSize, setTerminalFontSize, fontSizeLoading] = useSettingsState(
    "terminal_font_size",
    "",
    Settings.terminalFontSize,
    Settings.terminalFontSize,
  );
  const [terminalGl, setTerminalGl, glLoading] = useSettingsState(
    "terminal_gl",
    false,
    Settings.terminalGL,
    Settings.terminalGL,
  );
  const [terminalGhostty, setTerminalGhostty, ghosttyLoading] = useSettingsState(
    "terminal_ghostty",
    false,
    Settings.terminalGhostty,
    Settings.terminalGhostty,
  );
  const [terminalShell, setTerminalShell, shellLoading] = useSettingsState(
    "terminal_shell",
    "",
    Settings.terminalShell,
    Settings.terminalShell,
  );
  const [scriptShell, setScriptShell, scriptShellLoading] = useSettingsState(
    "script_shell",
    "",
    Settings.scriptShell,
    Settings.scriptShell,
  );
  const [prometheusUrl, setPrometheusUrl, urlLoading] = useSettingsState(
    "prometheus_url",
    "http://localhost:9090",
    Settings.runbookPrometheusUrl,
    Settings.runbookPrometheusUrl,
  );

  if (
    fontLoading ||
    glLoading ||
    ghosttyLoading ||
    urlLoading ||
    fontSizeLoading ||
    shellLoading ||
    scriptShellLoading ||
    !fonts
  )
    return <Spinner />;

  return (
    <>
      <Card shadow="sm">
        <CardBody className="flex flex-col gap-4">
          <h2 className="text-xl font-semibold">Terminal</h2>
          <div className="flex flex-row gap-4">
            <Autocomplete
              label="Terminal font"
              selectedKey={terminalFont}
              onSelectionChange={setTerminalFont}
              description="Font to use for the terminal"
              defaultItems={fonts.map((font) => ({ label: font, key: font }))}
            >
              {(item) => <AutocompleteItem key={item.key}>{item.label}</AutocompleteItem>}
            </Autocomplete>
            <div>
              <Input
                type="number"
                value={terminalFontSize || Settings.DEFAULT_FONT_SIZE}
                onChange={(e) => setTerminalFontSize(parseInt(e.target.value))}
                label="Font Size"
              />
            </div>
          </div>
          <SettingSwitch
            label="Use Ghostty terminal"
            isSelected={terminalGhostty}
            onValueChange={setTerminalGhostty}
            description="Experimental: Use Ghostty's WASM-based terminal emulator"
          />
          {!terminalGhostty && (
            <SettingSwitch
              label="Enable WebGL rendering"
              isSelected={terminalGl}
              onValueChange={setTerminalGl}
              description="May have issues with some fonts"
            />
          )}
          <SettingInput
            type="text"
            label="Custom shell"
            value={terminalShell || ""}
            onChange={setTerminalShell}
            placeholder="/bin/bash, /bin/zsh, /usr/bin/fish"
            description="Leave empty to use your default shell"
          />
        </CardBody>
      </Card>

      <Card shadow="sm">
        <CardBody className="flex flex-col gap-4">
          <h2 className="text-xl font-semibold">Script</h2>
          <p className="text-sm text-default-500">Configure default settings for script blocks</p>

          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <div>
                <label className="text-sm font-medium">Default shell</label>
                <p className="text-xs text-default-500">
                  Default shell interpreter for new script blocks
                </p>
              </div>
              <InterpreterSelector
                interpreter={scriptShell || systemDefaultShell}
                onInterpreterChange={setScriptShell}
                size="sm"
                variant="flat"
              />
            </div>
          </div>

          <div className="border-t pt-4">
            <p className="text-sm text-default-500 mb-3">
              Add custom script interpreters for use in script blocks
            </p>
          </div>

          <div className="flex flex-col gap-3">
            {scriptInterpreters.map((interpreter) => (
              <div
                key={interpreter.command}
                className="flex items-center justify-between p-2 border rounded-md"
              >
                <div>
                  <div className="font-medium">{interpreter.name}</div>
                  <div className="text-small text-default-500">{interpreter.command}</div>
                </div>
                <Button
                  isIconOnly
                  size="sm"
                  variant="light"
                  color="danger"
                  onPress={() => removeScriptInterpreter(interpreter.command)}
                >
                  <TrashIcon size={16} />
                </Button>
              </div>
            ))}
          </div>

          <div className="flex flex-row gap-2 mt-2">
            <Input
              placeholder="Display name (e.g., Ruby)"
              value={newInterpreterName}
              onValueChange={setNewInterpreterName}
              size="sm"
            />
            <Input
              placeholder="Command (e.g., /usr/bin/ruby -e)"
              value={newInterpreterCommand}
              onValueChange={setNewInterpreterCommand}
              size="sm"
            />
            <Button
              isIconOnly
              color="primary"
              onPress={addScriptInterpreter}
              isDisabled={!newInterpreterCommand || !newInterpreterName}
            >
              <PlusIcon size={16} />
            </Button>
          </div>
        </CardBody>
      </Card>

      <Card shadow="sm">
        <CardBody className="flex flex-col gap-4">
          <h2 className="text-xl font-semibold">Prometheus</h2>
          <SettingInput
            type="url"
            label="Default Prometheus server URL"
            value={prometheusUrl}
            onChange={setPrometheusUrl}
            placeholder="http://localhost:9090"
            description="Default URL for Prometheus blocks"
          />
        </CardBody>
      </Card>
    </>
  );
};

type AuthTokenModalProps = {
  onSubmit: (token: string) => void;
  onClose: () => void;
  open: boolean;
};

const AuthTokenModal = (props: AuthTokenModalProps) => {
  const [token, setToken] = useState("");
  const [validToken, setValidToken] = useState(false);

  useEffect(() => {
    const trimmed = token.trim();
    const valid = trimmed.startsWith("atapi_") && trimmed.length >= 20;
    setValidToken(valid);
  }, [token]);

  return (
    <Modal isOpen={props.open} onClose={() => props.onClose()} size="lg">
      <ModalContent>
        <ModalHeader>Log in via auth token</ModalHeader>
        <ModalBody>
          <Input
            type="password"
            label="Paste your token here"
            value={token}
            onValueChange={setToken}
          />
        </ModalBody>
        <ModalFooter>
          <Button
            isDisabled={!validToken}
            color="success"
            variant="flat"
            onPress={() => props.onSubmit(token.trim())}
          >
            Submit
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
};

// Sound types and helpers
interface SoundInfo {
  id: string;
  name: string;
}

async function loadSounds(): Promise<SoundInfo[]> {
  try {
    return await invoke<SoundInfo[]>("list_sounds");
  } catch (e) {
    console.warn("Failed to load sounds:", e);
    return [];
  }
}

type SoundOption = "none" | "chime" | string;
type OsOption = "always" | "not_focused" | "never";

interface NotificationRowProps {
  label: string;
  durationLabel: string;
  duration: number;
  onDurationChange: (val: number) => void;
  sound: SoundOption;
  onSoundChange: (val: SoundOption) => void;
  os: OsOption;
  onOsChange: (val: OsOption) => void;
  sounds: SoundInfo[];
  volume: number;
}

const NotificationRow = ({
  label,
  durationLabel,
  duration,
  onDurationChange,
  sound,
  onSoundChange,
  os,
  onOsChange,
  sounds,
  volume,
}: NotificationRowProps) => {
  const playSound = (soundId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    if (soundId === "none") return;

    console.log("Playing sound", soundId, "at volume", volume, "->", volume / 100);
    invoke("play_sound", { soundId, volume: volume / 100 }).catch((err) => {
      console.error("Failed to play sound:", err);
    });
  };

  const allSounds = [{ id: "none", name: "None" }, ...sounds];

  return (
    <div className="flex flex-col gap-2 py-3 border-b last:border-b-0">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-sm">{label}</span>
        <Input
          type="number"
          size="sm"
          className="w-16"
          value={duration.toString()}
          onChange={(e) => onDurationChange(parseInt(e.target.value) || 0)}
          min={0}
          max={3600}
          aria-label={durationLabel}
        />
        <span className="text-sm text-default-500">seconds</span>
      </div>
      <div className="flex items-center gap-4 pl-4">
        <Select
          label="Sound"
          size="sm"
          className="w-48"
          selectedKeys={[sound]}
          onSelectionChange={(keys) => {
            const key = keys.currentKey as SoundOption;
            if (key) onSoundChange(key);
          }}
          items={allSounds}
        >
          {(item) => (
            <SelectItem
              key={item.id}
              endContent={
                item.id !== "none" ? (
                  <button
                    className="p-1 hover:bg-default-200 rounded"
                    onClick={(e) => playSound(item.id, e)}
                  >
                    <PlayIcon size={14} />
                  </button>
                ) : null
              }
            >
              {item.name}
            </SelectItem>
          )}
        </Select>
        <Select
          label="System Notification"
          size="sm"
          className="w-52"
          selectedKeys={[os]}
          onSelectionChange={(keys) => {
            const key = keys.currentKey as OsOption;
            if (key) onOsChange(key);
          }}
        >
          <SelectItem key="always">Always</SelectItem>
          <SelectItem key="not_focused">When app not focused</SelectItem>
          <SelectItem key="never">Never</SelectItem>
        </Select>
      </div>
    </div>
  );
};

const NotificationSettings = () => {
  const sounds = useAsyncData(loadSounds);

  const [notificationsEnabled, setNotificationsEnabled, enabledLoading] = useSettingsState(
    "notifications_enabled",
    true,
    Settings.notificationsEnabled,
    Settings.notificationsEnabled,
  );

  const [volume, setVolume, volumeLoading] = useSettingsState(
    "notifications_volume",
    80,
    Settings.notificationsVolume,
    Settings.notificationsVolume,
  );

  // Block finished settings
  const [blockFinishedDuration, setBlockFinishedDuration, bfDurationLoading] = useSettingsState(
    "block_finished_duration",
    5,
    Settings.notificationsBlockFinishedDuration,
    Settings.notificationsBlockFinishedDuration,
  );
  const [blockFinishedSound, setBlockFinishedSound, bfSoundLoading] = useSettingsState(
    "block_finished_sound",
    "that_was_quick",
    Settings.notificationsBlockFinishedSound,
    Settings.notificationsBlockFinishedSound,
  );
  const [blockFinishedOs, setBlockFinishedOs, bfOsLoading] = useSettingsState(
    "block_finished_os",
    "not_focused",
    Settings.notificationsBlockFinishedOs,
    Settings.notificationsBlockFinishedOs,
  );

  // Block failed settings
  const [blockFailedDuration, setBlockFailedDuration, bxDurationLoading] = useSettingsState(
    "block_failed_duration",
    1,
    Settings.notificationsBlockFailedDuration,
    Settings.notificationsBlockFailedDuration,
  );
  const [blockFailedSound, setBlockFailedSound, bxSoundLoading] = useSettingsState(
    "block_failed_sound",
    "out_of_nowhere",
    Settings.notificationsBlockFailedSound,
    Settings.notificationsBlockFailedSound,
  );
  const [blockFailedOs, setBlockFailedOs, bxOsLoading] = useSettingsState(
    "block_failed_os",
    "always",
    Settings.notificationsBlockFailedOs,
    Settings.notificationsBlockFailedOs,
  );

  // Serial finished settings
  const [serialFinishedDuration, setSerialFinishedDuration, sfDurationLoading] = useSettingsState(
    "serial_finished_duration",
    0,
    Settings.notificationsSerialFinishedDuration,
    Settings.notificationsSerialFinishedDuration,
  );
  const [serialFinishedSound, setSerialFinishedSound, sfSoundLoading] = useSettingsState(
    "serial_finished_sound",
    "gracefully",
    Settings.notificationsSerialFinishedSound,
    Settings.notificationsSerialFinishedSound,
  );
  const [serialFinishedOs, setSerialFinishedOs, sfOsLoading] = useSettingsState(
    "serial_finished_os",
    "not_focused",
    Settings.notificationsSerialFinishedOs,
    Settings.notificationsSerialFinishedOs,
  );

  // Serial failed settings
  const [serialFailedDuration, setSerialFailedDuration, sxDurationLoading] = useSettingsState(
    "serial_failed_duration",
    0,
    Settings.notificationsSerialFailedDuration,
    Settings.notificationsSerialFailedDuration,
  );
  const [serialFailedSound, setSerialFailedSound, sxSoundLoading] = useSettingsState(
    "serial_failed_sound",
    "unexpected",
    Settings.notificationsSerialFailedSound,
    Settings.notificationsSerialFailedSound,
  );
  const [serialFailedOs, setSerialFailedOs, sxOsLoading] = useSettingsState(
    "serial_failed_os",
    "always",
    Settings.notificationsSerialFailedOs,
    Settings.notificationsSerialFailedOs,
  );

  // Serial paused settings
  const [serialPausedDuration, setSerialPausedDuration, spDurationLoading] = useSettingsState(
    "serial_paused_duration",
    0,
    Settings.notificationsSerialPausedDuration,
    Settings.notificationsSerialPausedDuration,
  );
  const [serialPausedSound, setSerialPausedSound, spSoundLoading] = useSettingsState(
    "serial_paused_sound",
    "to_the_point",
    Settings.notificationsSerialPausedSound,
    Settings.notificationsSerialPausedSound,
  );
  const [serialPausedOs, setSerialPausedOs, spOsLoading] = useSettingsState(
    "serial_paused_os",
    "not_focused",
    Settings.notificationsSerialPausedOs,
    Settings.notificationsSerialPausedOs,
  );

  const isLoading =
    sounds === null ||
    enabledLoading ||
    volumeLoading ||
    bfDurationLoading ||
    bfSoundLoading ||
    bfOsLoading ||
    bxDurationLoading ||
    bxSoundLoading ||
    bxOsLoading ||
    sfDurationLoading ||
    sfSoundLoading ||
    sfOsLoading ||
    sxDurationLoading ||
    sxSoundLoading ||
    sxOsLoading ||
    spDurationLoading ||
    spSoundLoading ||
    spOsLoading;

  if (isLoading) return <Spinner />;

  return (
    <Card shadow="sm">
      <CardBody className="flex flex-col gap-4">
        <h2 className="text-xl font-semibold">Notifications</h2>
        <p className="text-sm text-default-500">Get notified when blocks and workflows complete</p>

        <SettingSwitch
          label="Enable notifications"
          isSelected={notificationsEnabled}
          onValueChange={setNotificationsEnabled}
          description="Enable or disable all notifications"
        />

        {notificationsEnabled && (
          <>
            <Slider
              label="Volume"
              size="sm"
              step={1}
              minValue={0}
              maxValue={100}
              value={volume}
              onChange={(val: number | number[]) => {
                const numVal = Array.isArray(val) ? val[0] : val;
                setVolume(numVal);
              }}
              className="max-w-md"
            />

            <div className="border-t pt-4 mt-2">
              <p className="text-sm font-medium mb-2">Block Notifications</p>
              <NotificationRow
                label="Finished after running at least"
                durationLabel="Block finished minimum duration"
                duration={blockFinishedDuration}
                onDurationChange={setBlockFinishedDuration}
                sound={blockFinishedSound}
                onSoundChange={setBlockFinishedSound}
                os={blockFinishedOs}
                onOsChange={setBlockFinishedOs}
                sounds={sounds}
                volume={volume}
              />
              <NotificationRow
                label="Failed after running at least"
                durationLabel="Block failed minimum duration"
                duration={blockFailedDuration}
                onDurationChange={setBlockFailedDuration}
                sound={blockFailedSound}
                onSoundChange={setBlockFailedSound}
                os={blockFailedOs}
                onOsChange={setBlockFailedOs}
                sounds={sounds}
                volume={volume}
              />
            </div>

            <div className="border-t pt-4 mt-2">
              <p className="text-sm font-medium mb-2">Serial Execution Notifications</p>
              <NotificationRow
                label="Workflow finishes after running at least"
                durationLabel="Serial finished minimum duration"
                duration={serialFinishedDuration}
                onDurationChange={setSerialFinishedDuration}
                sound={serialFinishedSound}
                onSoundChange={setSerialFinishedSound}
                os={serialFinishedOs}
                onOsChange={setSerialFinishedOs}
                sounds={sounds}
                volume={volume}
              />
              <NotificationRow
                label="Workflow fails after running at least"
                durationLabel="Serial failed minimum duration"
                duration={serialFailedDuration}
                onDurationChange={setSerialFailedDuration}
                sound={serialFailedSound}
                onSoundChange={setSerialFailedSound}
                os={serialFailedOs}
                onOsChange={setSerialFailedOs}
                sounds={sounds}
                volume={volume}
              />
              <NotificationRow
                label="Workflow pauses after running at least"
                durationLabel="Serial paused minimum duration"
                duration={serialPausedDuration}
                onDurationChange={setSerialPausedDuration}
                sound={serialPausedSound}
                onSoundChange={setSerialPausedSound}
                os={serialPausedOs}
                onOsChange={setSerialPausedOs}
                sounds={sounds}
                volume={volume}
              />
            </div>
          </>
        )}
      </CardBody>
    </Card>
  );
};

const AISettings = () => {
  const aiEnabled = useStore((state) => state.aiEnabled);
  const aiShareContext = useStore((state) => state.aiShareContext);
  const setAiEnabled = useStore((state) => state.setAiEnabled);
  const setAiShareContext = useStore((state) => state.setAiShareContext);

  return (
    <>
      <Card shadow="sm">
        <CardBody className="flex flex-col gap-4 mb-4">
          <h2 className="text-xl font-semibold">AI</h2>
          <p className="text-sm text-default-500">
            Configure AI-powered features in runbooks
          </p>

          <SettingSwitch
            label="Enable AI features"
            isSelected={aiEnabled}
            onValueChange={setAiEnabled}
            description="Enable AI block generation and editing (Cmd+Enter, Cmd+K, AI Agent Sidebar)"
          />

          {aiEnabled && (
            <SettingSwitch
              className="ml-4"
              label="Share document context"
              isSelected={aiShareContext}
              onValueChange={setAiShareContext}
              description="Send document content to improve AI suggestions. Disable for sensitive documents."
            />
          )}
        </CardBody>
      </Card>
      {aiEnabled && (
        <>
          <AgentSettings />
          <AIOllamaSettings />
        </>
      )}
    </>
  );
};

const AgentSettings = () => {
  const providers = [
    ["Atuin Hub", "atuinhub"],
    ["Ollama", "ollama"]
  ]

  const [aiProvider, setAiProvider, aiProviderLoading] = useSettingsState(
    "ai_provider",
    "atuinhub",
    Settings.aiAgentProvider,
    Settings.aiAgentProvider,
  );

  const handleProviderChange = (keys: SharedSelection) => {
    const key = keys.currentKey as string;
    if (key) {
      setAiProvider(key);
    }
  };

  return (
    <Card shadow="sm">
      <CardBody className="flex flex-col gap-4">
        <h2 className="text-xl font-semibold">AI Agent</h2>

        <Select
          label="Default AI provider"
          value={aiProvider}
          onSelectionChange={handleProviderChange}
          className="mt-4"
          placeholder="Select default AI provider"
          selectedKeys={[aiProvider]}
          items={providers.map(([name, id]) => ({ label: name, key: id }))}
          isDisabled={aiProviderLoading}
        >
          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
        </Select>
      </CardBody>
    </Card>
  );
};

const AIOllamaSettings = () => {
  const [ollamaSettings, setOllamaSettings, isLoading] = useAIProviderSettings<OllamaSettings>("ollama", {
    enabled: false,
    endpoint: "http://localhost:11434",
    model: "",
  });

  return (
    <Card shadow="sm">
      <CardBody className="flex flex-col gap-4">
        <h2 className="text-xl font-semibold">Ollama</h2>

        <SettingSwitch
          label="Enable Ollama AI provider"
          isSelected={ollamaSettings.enabled}
          onValueChange={(enabled) => setOllamaSettings({ ...ollamaSettings, enabled })}
          description="Toggle to use Ollama as the AI provider."
        />

        {ollamaSettings.enabled && (
          <div className="flex flex-col gap-4">
            <Input
              label="Endpoint (optional, defaults to http://localhost:11434)"
              placeholder="Endpoint URL (e.g. http://localhost:11434)"
              value={ollamaSettings.endpoint}
              onValueChange={(value) => setOllamaSettings({ ...ollamaSettings, endpoint: value })}
              isDisabled={isLoading}
            />

            <Input
              label="Model (required; your chosen model must support tool calling)"
              placeholder="Model name"
              value={ollamaSettings.model}
              onValueChange={(value) => setOllamaSettings({ ...ollamaSettings, model: value })}
              isDisabled={isLoading}
            />
          </div>
        )}
      </CardBody>
    </Card>
  );
};

const UserSettings = () => {
  const user = useStore((state) => state.user);
  const refreshUser = useStore((state) => state.refreshUser);
  const { isOpen: modalOpen, onOpen: openModal, onClose: closeModal } = useDisclosure();

  async function logOut() {
    await api.clearHubApiToken();
    SocketManager.setApiToken(null);
    refreshUser();
  }

  function handleTokenSubmit(token: string) {
    closeModal();
    const deepLink = `atuin://register-token/${token}`;
    // token submit deep link doesn't require a runbook activation,
    // so passing an empty function for simplicity
    handleDeepLink(deepLink, () => { });
  }

  let content;
  if (!user || !user.isLoggedIn()) {
    content = (
      <>
        <p>You are not logged in.</p>
        <div className="flex flex-row gap-2 items-center">
          <Button
            onPress={() => open(AtuinEnv.url("/settings/desktop-connect"))}
            color="success"
            variant="flat"
            className="grow"
          >
            Log in via Atuin Hub
          </Button>
          or
          <Button onPress={() => openModal()} color="primary" variant="flat" className="grow">
            Log in via auth token
          </Button>
        </div>
      </>
    );
  } else {
    content = (
      <>
        <User
          name={""}
          avatarProps={{ src: user.avatar_url || undefined }}
          description={
            <Link
              isExternal
              href={AtuinEnv.url(`/${user.username}`)}
              onPress={() => {
                open(AtuinEnv.url(`/${user.username}`));
              }}
            >
              {user.username}
            </Link>
          }
          classNames={{ base: "mt-2 justify-start" }}
        />
        <Button onPress={logOut} color="danger" variant="flat">
          Sign out
        </Button>
      </>
    );
  }

  return (
    <Card shadow="sm">
      <CardBody>
        <h2 className="text-xl font-semibold">User</h2>
        <div className="flex flex-col gap-4">{content}</div>
        {modalOpen && (
          <AuthTokenModal onSubmit={handleTokenSubmit} onClose={closeModal} open={modalOpen} />
        )}
      </CardBody>
    </Card>
  );
};

// Main Settings component
const SettingsPanel = () => {
  return (
    <div className="flex flex-col gap-4 p-4 pt-2 w-full">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-small text-default-400 uppercase font-semibold">
          Customize your experience
        </p>
      </div>
      <Tabs
        aria-label="Settings"
        color="primary"
        classNames={{
          tabList: "sticky top-4 start-0 z-20 pt-2 pb-4",
          panel: "w-full",
        }}
        isVertical
      >
        <Tab key="general" title="General">
          <div className="flex flex-col gap-4">
            <GeneralSettings />
          </div>
        </Tab>

        <Tab key="runbook" title="Runbooks">
          <div className="flex flex-col gap-4">
            <RunbookSettings />
          </div>
        </Tab>

        <Tab key="notification" title="Notifications">
          <div className="flex flex-col gap-4">
            <NotificationSettings />
          </div>
        </Tab>

        <Tab key="ai" title="AI">
          <div className="flex flex-col gap-4">
            <AISettings />
          </div>
        </Tab>

        <Tab key="user" title="User">
          <div className="flex flex-col gap-4">
            <UserSettings />
          </div>
        </Tab>
      </Tabs>
    </div>
  );
};

export default SettingsPanel;
