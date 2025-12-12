import { RemoteRunbook } from "@/state/models";
import Runbook, { RunbookVisibility } from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import { Button, Input, Select, SelectItem, SharedSelection, Tooltip } from "@heroui/react";
import { CheckIcon, CircleQuestionMarkIcon, PencilIcon, SettingsIcon, XIcon } from "lucide-react";
import { useMemo, useReducer, useRef } from "react";
import * as api from "@/api/api";
import { cn } from "@/lib/utils";
import CollaborationManager from "@/routes/runbooks/CollaborationManager";
import { open } from "@tauri-apps/plugin-shell";

interface RunbookControlsProps {
  runbook: Runbook;
  remoteRunbook?: RemoteRunbook;
  isOrgOwned: boolean;
  isOfflineRunbook: boolean;
  onClose: () => void;
}

interface EditState<T> {
  value: T;
  lastTrueValue: T;
  mode: "idle" | "editing" | "loading";
  fieldDisabled: boolean;
  error?: string;
}

type EditStateAction<T> =
  | {
      type: "edit";
    }
  | {
      type: "cancel";
    }
  | {
      type: "set-loading";
      loading: boolean;
      newState?: "idle" | "editing";
    }
  | {
      type: "set-value";
      value: T;
    }
  | {
      type: "set-true-value";
      value: T;
    }
  | {
      type: "set-error";
      error: string;
    };

function editStateReducer<T>(state: EditState<T>, action: EditStateAction<T>): EditState<T> {
  switch (action.type) {
    case "edit":
      return { ...state, mode: "editing", fieldDisabled: false };
    case "cancel":
      return {
        ...state,
        mode: "idle",
        fieldDisabled: true,
        value: state.lastTrueValue,
        error: undefined,
      };
    case "set-loading":
      return {
        ...state,
        mode: action.loading ? "loading" : action.newState || "idle",
        fieldDisabled: true,
      };
    case "set-value":
      return { ...state, value: action.value, error: undefined };
    case "set-true-value":
      return {
        ...state,
        lastTrueValue: action.value,
        mode: "idle",
        fieldDisabled: true,
        error: undefined,
      };
    case "set-error":
      return { ...state, error: action.error, fieldDisabled: false };
    default:
      const x: never = action;
      console.error("Unknown action", x);
      return state;
  }
}

interface EditStateApi<T> {
  startEditing: () => void;
  cancelEditing: () => void;
  setValue: (value: T) => void;
  submitValue: () => void;
}

function useEditState<T>(
  initialValue: T,
  submitValue: (value: T) => Promise<T>,
  inputRef?: React.RefObject<any>,
): [EditState<T>, EditStateApi<T>] {
  const [state, dispatch] = useReducer(editStateReducer<T>, {
    value: initialValue,
    lastTrueValue: initialValue,
    mode: "idle",
    fieldDisabled: true,
    error: undefined,
  });

  const api = useMemo(() => {
    return {
      startEditing: () => {
        dispatch({ type: "edit" });
        setTimeout(() => {
          if (inputRef?.current) {
            inputRef.current.focus();
          }
        }, 10);
      },
      cancelEditing: () => dispatch({ type: "cancel" }),
      setValue: (value: T) => dispatch({ type: "set-value", value }),
      submitValue: () => {
        dispatch({ type: "set-loading", loading: true });
        submitValue(state.value)
          .then((value) => {
            dispatch({ type: "set-true-value", value: value });
          })
          .catch((error) => {
            let message = error.message;
            if (error.data && error.data.errors) {
              message = error.data.errors[0];
            }

            dispatch({ type: "set-loading", loading: false, newState: "editing" });
            dispatch({ type: "set-error", error: message });
            setTimeout(() => {
              if (inputRef?.current) {
                inputRef.current.focus();
              }
            }, 10);
          });
      },
    };
  }, [dispatch, submitValue, state.value]);

  return [state, api];
}

interface RowEditButtonProps {
  disabled: boolean;
  state: EditState<any>;
  api: EditStateApi<any>;
}

function RowEditButton(props: RowEditButtonProps) {
  return (
    <>
      <Button
        isIconOnly
        variant="flat"
        isDisabled={props.disabled || props.state.mode === "loading"}
        onPress={props.state.mode === "idle" ? props.api.startEditing : props.api.submitValue}
        className="mr-2"
        color={props.state.mode === "idle" ? "default" : "success"}
        isLoading={props.state.mode === "loading"}
      >
        {props.state.mode === "idle" && <PencilIcon className="h-5 w-5" />}
        {props.state.mode !== "idle" && <CheckIcon className="h-5 w-5" />}
      </Button>
      <Button
        isIconOnly
        variant="flat"
        isDisabled={props.disabled || props.state.mode === "loading"}
        onPress={props.api.cancelEditing}
        className={cn(
          "invisible",
          (props.state.mode === "editing" || props.state.mode === "loading") && "visible",
        )}
        color="danger"
      >
        <XIcon className="h-5 w-5" />
      </Button>
    </>
  );
}

export default function RunbookControls(props: RunbookControlsProps) {
  const connectionState = useStore((state) => state.connectionState);
  const disabled = connectionState !== ConnectionState.Online;

  const tableCell = "p-2";
  const tableCellRight = tableCell + " text-right mt-2";
  const tableCellButton = tableCell + " text-left";

  const slugInputRef = useRef<HTMLInputElement>(null);
  const visibilitySelectRef = useRef<HTMLSelectElement>(null);

  const [slugState, slugApi] = useEditState(
    props.remoteRunbook?.slug || "",
    async (slug) => {
      await api.updateRunbook(props.runbook, slug, visibilityState.lastTrueValue);
      return slug;
    },
    slugInputRef,
  );

  const [visibilityState, visibilityApi] = useEditState(
    props.remoteRunbook?.visibility || "public",
    async (visibility) => {
      await api.updateRunbook(props.runbook, slugState.lastTrueValue, visibility);
      return visibility;
    },
    visibilitySelectRef,
  );

  function handleVisibilityChange(keys: SharedSelection) {
    visibilityApi.setValue(keys.currentKey as RunbookVisibility);
  }

  function handleLearnMore(e: React.MouseEvent<HTMLAnchorElement>) {
    e.preventDefault();
    open("https://docs.atuin.sh/desktop/workspaces/");
  }

  function handleSlugKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      slugApi.submitValue();
    }
  }

  return (
    <div className="flex flex-col w-full p-2 border-b">
      <div className="flex items-center justify-between select-none">
        <div className="flex items-center cursor-default">
          <SettingsIcon className="w-4 h-4 inline-block mr-1" />
          Runbook settings
        </div>
        <Button isIconOnly variant="faded" size="sm" onPress={props.onClose}>
          <XIcon className="w-4 h-4" />
        </Button>
      </div>

      {props.isOfflineRunbook && (
        <p className="text-sm text-gray-500 dark:text-gray-400 italic ml-5 mt-1 select-none cursor-default">
          Runbooks in offline workspaces exist only on your device, and cannot be shared via the
          Hub.{" "}
          <a
            href="https://docs.atuin.sh/desktop/workspaces/"
            className="text-blue-500"
            onClick={handleLearnMore}
          >
            Learn more
          </a>
        </p>
      )}
      {disabled && !props.isOfflineRunbook && (
        <p className="text-sm text-gray-500 dark:text-gray-400 italic ml-5 mt-1 select-none cursor-default">
          Runbook slug, visibility, and other sharing settings can only be modified while online and
          logged in to Atuin Hub
        </p>
      )}
      <div className="grid grid-cols-[minmax(5rem,_15rem)_minmax(200px,_3fr)_8rem] w-full mt-2">
        {props.remoteRunbook && (
          <>
            <div className={tableCellRight}>
              <Tooltip content="The runbook slug is used to generate the runbook URL" showArrow>
                <CircleQuestionMarkIcon className="w-4 h-4 inline-block mr-1 mb-1" />
              </Tooltip>
              Runbook slug:
            </div>
            <div className={tableCell}>
              <Input
                ref={slugInputRef}
                value={slugState.value}
                onValueChange={slugApi.setValue}
                minLength={2}
                maxLength={255}
                isDisabled={disabled || slugState.fieldDisabled}
                isInvalid={!!slugState.error}
                errorMessage={slugState.error}
                classNames={{
                  base: "w-full",
                }}
                autoCapitalize="off"
                autoComplete="off"
                autoCorrect="off"
                onKeyDown={handleSlugKeyDown}
              />
            </div>
            <div className={tableCellButton}>
              <RowEditButton disabled={disabled} state={slugState} api={slugApi} />
            </div>
          </>
        )}
        {props.remoteRunbook && (
          <>
            <div className={tableCellRight}>
              <Tooltip
                content="The runbook visibility determines who can view the runbook on Atuin Hub"
                showArrow
              >
                <CircleQuestionMarkIcon className="w-4 h-4 inline-block mr-1 mb-1" />
              </Tooltip>
              Runbook visibility:
            </div>
            <div className={tableCell}>
              <Select
                ref={visibilitySelectRef}
                onSelectionChange={handleVisibilityChange}
                selectionMode="single"
                selectedKeys={new Set([visibilityState.value as string])}
                isDisabled={disabled || visibilityState.fieldDisabled}
                disallowEmptySelection
              >
                <SelectItem key="public" textValue="Public">
                  Public
                </SelectItem>
                <SelectItem key="unlisted" textValue="Unlisted">
                  Unlisted
                </SelectItem>
                <SelectItem key="private" textValue="Private">
                  Private
                </SelectItem>
              </Select>
            </div>
            <div className={tableCellButton}>
              <RowEditButton disabled={disabled} state={visibilityState} api={visibilityApi} />
            </div>
          </>
        )}
        {props.remoteRunbook && !props.isOrgOwned && (
          <>
            <div className={tableCellRight}>Collaborators:</div>
            <div className={tableCell}>
              <CollaborationManager
                runbook={props.runbook}
                remoteRunbook={props.remoteRunbook}
                disabled={disabled}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
