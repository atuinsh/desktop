import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { PlusIcon, TrashIcon } from "lucide-react";
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

async function loadFonts(): Promise<string[]> {
  const fonts = await invoke<string[]>("list_fonts");
  fonts.push("Inter");
  fonts.push("FiraCode");
  fonts.sort();
  return fonts;
}

// Custom hook for managing settings
const useSettingsState = (
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
}

const SettingSwitch = ({
  label,
  isSelected,
  onValueChange,
  description,
  className,
}: SettingsSwitchProps) => (
  <Switch
    isSelected={isSelected}
    onValueChange={onValueChange}
    className={cn("flex justify-between items-center w-full", className)}
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

  if (
    isLoading ||
    vimModeLoading ||
    shellCheckEnabledLoading ||
    shellCheckPathLoading
  ) return <Spinner />;

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

  // Load script interpreters
  useEffect(() => {
    Settings.scriptInterpreters().then((interpreters) => {
      setScriptInterpreters(interpreters);
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
            label="Enable WebGL rendering"
            isSelected={terminalGl}
            onValueChange={setTerminalGl}
            description="May have issues with some fonts"
          />
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
                interpreter={scriptShell || "zsh"}
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
            label="Prometheus server URL"
            value={prometheusUrl}
            onChange={setPrometheusUrl}
            placeholder="http://localhost:9090"
            description="URL for querying metrics (can be overridden per-block)"
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
    const valid = token.length == 54 && token.startsWith("atapi_");
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
            onPress={() => props.onSubmit(token)}
          >
            Submit
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
};

const AISettings = () => {
  const [aiEnabled, setAiEnabled, enabledLoading] = useSettingsState(
    "ai_enabled",
    false,
    Settings.aiEnabled,
    Settings.aiEnabled,
  );
  const [aiApiKey, setAiApiKey, keyLoading] = useSettingsState(
    "ai_api_key",
    "",
    Settings.aiApiKey,
    Settings.aiApiKey,
  );

  if (enabledLoading || keyLoading) return <Spinner />;

  return (
    <Card shadow="sm">
      <CardBody className="flex flex-col gap-4">
        <h2 className="text-xl font-semibold">AI Integration</h2>
        <p className="text-sm text-default-500">Configure AI-powered runbook generation</p>

        <SettingSwitch
          label="Enable AI features"
          isSelected={aiEnabled}
          onValueChange={setAiEnabled}
          description="Enable AI-powered runbook generation and assistance"
        />

        {aiEnabled && (
          <SettingInput
            type="password"
            label="Anthropic API Key"
            value={aiApiKey || ""}
            onChange={setAiApiKey}
            placeholder="sk-ant-api03-..."
            description="Your Anthropic API key for Claude models"
          />
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
    handleDeepLink(deepLink, () => {});
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
    <div className="flex flex-col gap-4 p-4 pt-2 w-full overflow-y-auto">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-small text-default-400 uppercase font-semibold">
          Customize your experience
        </p>
      </div>
      <div className="flex flex-col gap-4">
        <GeneralSettings />
        <RunbookSettings />
        <AISettings />
        <UserSettings />
      </div>
    </div>
  );
};

export default SettingsPanel;
