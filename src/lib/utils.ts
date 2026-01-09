import AtuinEnv from "@/atuin_env";
import { invoke } from "@tauri-apps/api/core";
import type { ClassValue } from "clsx";

import clsx from "clsx";
import { useEffect, useRef, useState } from "react";
import { extendTailwindMerge } from "tailwind-merge";
import { stringify } from "yaml";

export type { Option } from "@binarymuse/ts-stdlib";
export { None, Some } from "@binarymuse/ts-stdlib";

const COMMON_UNITS = ["small", "medium", "large"];

/**
 * We need to extend the tailwind merge to include NextUI's custom classes.
 *
 * So we can use classes like `text-small` or `text-default-500` and override them.
 */
const twMerge = extendTailwindMerge({
  extend: {
    theme: {
      opacity: ["disabled"],
      spacing: ["divider"],
      borderWidth: COMMON_UNITS,
      borderRadius: COMMON_UNITS,
    },
    classGroups: {
      shadow: [{ shadow: COMMON_UNITS }],
      "font-size": [{ text: ["tiny", ...COMMON_UNITS] }],
      "bg-image": ["bg-stripe-gradient"],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// edge still uses the old one
export function getWeekInfo() {
  let locale = new Intl.Locale(navigator.language);

  // @ts-ignore
  if (locale.getWeekInfo) {
    // @ts-ignore
    return locale.getWeekInfo();
    // @ts-ignore
  } else if (locale.weekInfo) {
    // @ts-ignore
    return locale.weekInfo;
  }

  throw new Error("Could not fetch week info via new or old api");
}

export function formatBytes(bytes: number): string {
  return new Intl.NumberFormat("en-US", {
    notation: "compact",
    style: "unit",
    unit: "byte",
    unitDisplay: "narrow",
  }).format(bytes);
}

export function formatDuration(ms: number) {
  if (ms < 2000) {
    return `${Math.round(ms)}ms`;
  }

  const seconds = Math.floor(ms / 1000);
  const milliseconds = ms % 1000;

  // For sub-2-minute durations, we display seconds with optional milliseconds
  // We floor the milliseconds first to avoid floating point imprecision
  // causing extra decimal places
  if (seconds < 120) {
    return `${seconds}${
      milliseconds ? `.${Math.floor(milliseconds).toString().padStart(3, "0")}` : ""
    }s`;
  }

  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;

  if (minutes < 60) {
    return `${minutes}m ${remainingSeconds}s`;
  }

  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;

  if (hours < 24) {
    return `${hours}h ${remainingMinutes}m ${remainingSeconds}s`;
  }

  const days = Math.floor(hours / 24);
  const remainingHours = hours % 24;

  return `${days}d ${remainingHours}h ${remainingMinutes}m ${remainingSeconds}s`;
}

export async function installAtuinCLI() {
  console.log("Installing CLI...");
  await invoke("install_cli");

  console.log("Setting up plugin...");
  await invoke("setup_cli");
}

/**
 * Sets state to true after a timeout, but resets the timer if the reset
 * function is called.

 * Returns a tuple of the debounced state, a function to reset the debounce,
 * and a function to clear the debounced state without restarting the timer.
 *
 * @param timeout How long to wait before setting the state to true
 */
export function useDebounce(
  timeout: number,
  initialValue = false,
): [boolean, () => void, () => void] {
  const ref = useRef<Timeout | null>(null);
  const [debounced, setDebouced] = useState<boolean>(initialValue);

  function resetDebounce() {
    setDebouced(false);
    if (ref.current) {
      clearTimeout(ref.current);
    }
    ref.current = setTimeout(() => {
      setDebouced(true);
    }, timeout);
  }

  function clearDebouce() {
    setDebouced(false);
    if (ref.current) {
      clearTimeout(ref.current);
    }
  }

  return [debounced, resetDebounce, clearDebouce];
}

/**
 * Stores the value in a ref and updates it when the value changes. Useful for
 * ensuring that a value is up-to-date in an async callback.
 */
export function useMemory<T>(value: T): React.RefObject<T> {
  const ref = useRef<T>(value);
  useEffect(() => {
    ref.current = value;
  }, [value]);

  return ref;
}

export function usePrevious<T>(value: T): T | undefined {
  const [current, setCurrent] = useState<T>(value);
  const [previous, setPrevious] = useState<T | undefined>(undefined);

  if (value !== current) {
    setPrevious(current);
    setCurrent(value);
  }

  return previous;
}

export function slugify(name: string | null): string {
  if (name) {
    return name
      .trim()
      .toLowerCase()
      .replace(/[\s\-]+/g, "-")
      .replace(/[^a-z0-9_\-]/gi, "");
  } else {
    return "";
  }
}

export function capitalize(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function timeoutPromise<T>(ms: number, resolveValue: T) {
  return new Promise<T>((resolve) => {
    setTimeout(() => resolve(resolveValue), ms);
  });
}

export function dbPath(filename: string) {
  return `${AtuinEnv.sqliteFilePrefix}${filename}`;
}

export function usernameFromNwo(nwo?: string): string | undefined {
  if (!nwo) {
    return undefined;
  }
  return nwo.split("/")[0];
}

export function normalizeInput(input: string) {
  return input
    .replace(/\u201c|\u201d/g, '"') // Replace opening/closing double quotes
    .replace(/\u2018|\u2019/g, "'"); // Replace opening/closing single quotes
}

export function usePromise<T, E = Error>(
  promise: Promise<T>,
  deps: any[] = [],
): [T | undefined, E | undefined] {
  const [value, setValue] = useState<T | undefined>(undefined);
  const [error, setError] = useState<E | undefined>(undefined);

  useEffect(() => {
    promise.then(setValue).catch((error) => setError(error as E));
  }, [promise, ...deps]);

  return [value, error];
}

export function useAsyncData<T>(fn: () => Promise<T>, deps: any[] = []) {
  const [value, setValue] = useState<T | null>(null);

  useEffect(() => {
    fn().then(setValue);
  }, [fn, ...deps]);

  return value;
}

export function toSnakeCase(str: string) {
  return (
    str
      // Replace any non-alphanumeric characters with spaces
      .replace(/[^\w\s]/g, " ")
      // Replace multiple spaces with a single space
      .replace(/\s+/g, " ")
      // Trim whitespace from beginning and end
      .trim()
      // Convert to lowercase
      .toLowerCase()
      // Replace spaces with underscores
      .replace(/\s/g, "_")
  );
}

export function exportPropMatter(type: string, props: any, exportProps: string[]) {
  let propMatter = exportProps.reduce(
    (acc: any, key: string) => {
      if (props[key] !== undefined) acc[key] = props[key as keyof typeof props];
      return acc;
    },
    { type },
  );

  return `---\n${stringify(propMatter)}---\n`;
}

export function withoutProperties<T extends object>(object: T, properties: (keyof T)[]) {
  const newObj = { ...object };
  for (const property of properties) {
    delete newObj[property];
  }
  return newObj;
}
