window.addEventListener("unhandledrejection", (event) => {
  console.error("Unhandled rejection", event);
});

// Initialize global types
import "./global";

// import sentry before anything else
import { init_tracking } from "./tracking";
import track_event from "./tracking";

import { event } from "@tauri-apps/api";
import { useEffect } from "react";
import ReactDOM from "react-dom/client";
import { HeroUIProvider, ToastProvider } from "@heroui/react";
import { QueryClientProvider } from "@tanstack/react-query";
import "./styles.css";

import * as api from "./api/api";
import SocketManager from "./socket";
import WorkspaceSyncManager from "./lib/sync/workspace_sync_manager";
import { useStore } from "@/state/store";
import ServerNotificationManager from "./server_notification_manager";
import { trackOnlineStatus } from "./lib/online_tracker";
import { setupColorModes } from "./lib/color_modes";
import { setupServerEvents } from "./lib/server_events";
import { invoke } from "@tauri-apps/api/core";
import debounce from "lodash.debounce";
import Workspace from "./state/runbooks/workspace";
import { SharedStateManager } from "./lib/shared_state/manager";
import { startup as startupOperationProcessor } from "./state/runbooks/operation_processor";

import ServerObserver from "./lib/sync/server_observer";
import DevConsole from "./lib/dev/dev_console";
import SSHBus from "./lib/buses/ssh";
import AppBus from "./lib/app/app_bus";
import Runbook from "./state/runbooks/runbook";
import Operation from "./state/runbooks/operation";
import EditorBus from "./lib/buses/editor";
import BlockBus from "./lib/workflow/block_bus";
import { generateBlocks } from "./lib/ai/block_generator";
import WorkspaceManager from "./lib/workspaces/manager";
import Root from "./routes/root/Root";
import RunbookBus from "./lib/app/runbook_bus";
import { grandCentral } from "./lib/events/grand_central";
import { AdvancedSettings } from "./rs-bindings/AdvancedSettings";

(async () => {
  try {
    const token = await api.getHubApiToken();
    SocketManager.setApiToken(token);
  } catch (_err) {
    console.warn("Not able to fetch Hub API token for socket manager");
  }
})();

// If the user has opted in, we will setup sentry/posthog
init_tracking();

const socketManager = SocketManager.get();
const notificationManager = ServerNotificationManager.get();
const workspaceSyncManager = WorkspaceSyncManager.get(useStore);
const queryClient = useStore.getState().queryClient;
const serverObserver = new ServerObserver(useStore, notificationManager);
RunbookBus.initialize();

const stateProxy = new Proxy(
  {},
  {
    get(_target, prop, _receiver) {
      const state = useStore.getState();
      return Reflect.get(state, prop, state);
    },
    set() {
      // Prevent direct state mutations
      return false;
    },
    has(_target, prop) {
      return prop in useStore.getState();
    },
    ownKeys() {
      return Reflect.ownKeys(useStore.getState());
    },
    getOwnPropertyDescriptor(_target, prop) {
      return Reflect.getOwnPropertyDescriptor(useStore.getState(), prop);
    },
  },
);

DevConsole.addAppObject("invoke", invoke)
  .addAppObject("useStore", useStore)
  .addAppObject("state", stateProxy) // app.state.user === app.useStore.getState().user
  .addAppObject("api", api)
  .addAppObject("serverObserver", serverObserver)
  .addAppObject("socketManager", socketManager)
  .addAppObject("notificationManager", notificationManager)
  .addAppObject("workspaceSyncManager", workspaceSyncManager)
  .addAppObject("workspaceManager", WorkspaceManager.getInstance())
  .addAppObject("queryClient", queryClient)
  .addAppObject("AppBus", AppBus.get())
  .addAppObject("SSHBus", SSHBus.get())
  .addAppObject("EditorBus", EditorBus.get())
  .addAppObject("BlockBus", BlockBus.get())
  .addAppObject("SharedStateManager", SharedStateManager)
  .addAppObject("generateBlocks", generateBlocks)
  .addAppObject("grandCentral", grandCentral)
  .addAppObject("models", {
    Runbook,
    Workspace,
    Operation,
  });

event.listen("tauri://blur", () => {
  useStore.getState().setFocused(false);
  track_event("app.blur");
});

event.listen("tauri://focus", () => {
  useStore.getState().setFocused(true);
  track_event("app.focus");
});

event.listen("tauri://close-requested", () => {
  track_event("app.close");
});

setupServerEvents(useStore, notificationManager);
setupColorModes(useStore);

trackOnlineStatus();
// When the socket connects or disconnects, re-check online status immediately
socketManager.onConnect(() => trackOnlineStatus());
socketManager.onDisconnect(() => trackOnlineStatus());

const debouncedSaveWindowInfo = debounce(async () => {
  invoke("save_window_info");
}, 500);

event.listen("tauri://move", debouncedSaveWindowInfo);
event.listen("tauri://resize", debouncedSaveWindowInfo);

function Application() {
  const { refreshUser, refreshCollaborations, online, user, uiScale } = useStore();

  useEffect(() => {
    if (online) {
      refreshUser();
    }
  }, [online]);

  useEffect(() => {
    if (online) {
      refreshCollaborations();
    }
  }, [online, user]);

  useEffect(() => {
    startupOperationProcessor();
  }, []);

  useEffect(() => {
    document.documentElement.style.zoom = `${uiScale}%`;
  }, [uiScale]);

  return (
    <HeroUIProvider>
      <ToastProvider
        placement="bottom-center"
        toastOffset={40}
        toastProps={{
          classNames: {
            base: "overflow-hidden",
            description: "break-all",
          },
        }}
      />
      <QueryClientProvider client={queryClient}>
        {/* <ReactQueryDevtools initialIsOpen={false} buttonPosition="bottom-right" /> */}
        <main className="text-foreground bg-background overflow-hidden z-20 relative">
          <Root />
        </main>
      </QueryClientProvider>
    </HeroUIProvider>
  );
}

async function setup() {
  try {
    const advancedSettings = await invoke<AdvancedSettings>("get_advanced_settings");
    useStore.getState().setAdvancedSettings(advancedSettings);
  } catch (err) {
    console.error("Failed to get advanced settings:", err);
  }

  invoke<void>("reset_workspaces");
  const currentVersion = await invoke<string>("get_app_version");
  useStore.getState().setCurrentVersion(currentVersion);
  try {
    await grandCentral.startListening();
  } catch (err) {
    console.warn("Failed to start Grand Central:", err);
    console.warn("Note: this is normal after a page refresh");
  }
}

(async () => {
  await setup();
  ReactDOM.createRoot(document.getElementById("root")!).render(<Application />);
  invoke("show_window");
})();

// Whelp, this is a weird one.
// We were finding that pressing "esc" inside a codemirror editor would collapse the *entire blocknote block*
// to some unprintable character. This could be undone with ctrl+z, but otherwise was unrecoverable.
// Capturing the escape key inside the codemirror editor prevented the issue, but then if you clicked
// inside another block and pressed escape, either the original block or the newly clicked block would
// collapse in the same way.
//
// This gross hack seems to fix that issue. ¯\_(ツ)_/¯
window.addEventListener(
  "keydown",
  (event) => {
    const blocknoteBlock = (event.target as Element).closest(".bn-block");
    const codemirrorEditor = (event.target as Element).closest(".cm-editor");

    // Allow Escape to propagate to CodeMirror editors (needed for vim mode)
    if (blocknoteBlock && event.key === "Escape" && !codemirrorEditor) {
      event.preventDefault();
      event.stopPropagation();
    }
  },
  {
    capture: true,
  },
);
