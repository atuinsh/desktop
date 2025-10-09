type OperatingSystem = "macos" | "windows" | "linux" | "testenv";

type GlobalOptions = {
  os: OperatingSystem;
  customTitleBar: boolean;
  devPrefix: string;
  noSync: boolean;
  channel: "stable" | "edge";
};

export function getGlobalOptions(): GlobalOptions {
  // If we're running a test, we don't have the window object
  if (typeof window === "undefined") {
    return {
      os: "testenv",
      customTitleBar: true,
      devPrefix: "dev",
      noSync: true,
      channel: "stable",
    };
  }

  const params = new URLSearchParams(window.location.search);

  return {
    os: params.get("os") as OperatingSystem,
    customTitleBar: params.get("os") === "macos",
    devPrefix: params.get("devPrefix") || "dev",
    noSync: params.get("noSync") === "true",
    channel: (params.get("channel") as "stable" | "edge") || "stable",
  } as const;
}
