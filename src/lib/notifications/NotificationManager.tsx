import { useEffect, useRef, useCallback } from "react";
import { addToast } from "@heroui/react";
import {
  grandCentral,
  GrandCentralEvents,
} from "@/lib/events/grand_central";
import { Settings } from "@/state/settings";
import Runbook from "@/state/runbooks/runbook";

interface BlockExecution {
  blockId: string;
  runbookId: string;
  startTime: number;
}

interface NotificationSettings {
  enabled: boolean;
  osEnabled: boolean;
  soundEnabled: boolean;
  toastEnabled: boolean;
  minDurationSecs: number;
  onlyWhenUnfocused: boolean;
  onFailure: "always" | "after_duration" | "never";
}

type NotificationPayload = {
  title: string;
  body: string;
  success: boolean;
  duration: number;
};

async function loadSettings(): Promise<NotificationSettings> {
  const [enabled, osEnabled, soundEnabled, toastEnabled, minDurationSecs, onlyWhenUnfocused, onFailure] =
    await Promise.all([
      Settings.notificationsEnabled(),
      Settings.notificationsOsEnabled(),
      Settings.notificationsSoundEnabled(),
      Settings.notificationsToastEnabled(),
      Settings.notificationsMinDurationSecs(),
      Settings.notificationsOnlyWhenUnfocused(),
      Settings.notificationsOnFailure(),
    ]);

  return { enabled, osEnabled, soundEnabled, toastEnabled, minDurationSecs, onlyWhenUnfocused, onFailure };
}

function playNotificationSound(success: boolean) {
  try {
    const audioContext = new AudioContext();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    if (success) {
      // Pleasant two-tone chime for success
      oscillator.frequency.setValueAtTime(523.25, audioContext.currentTime); // C5
      oscillator.frequency.setValueAtTime(659.25, audioContext.currentTime + 0.1); // E5
    } else {
      // Lower tone for failure
      oscillator.frequency.setValueAtTime(220, audioContext.currentTime); // A3
      oscillator.frequency.setValueAtTime(196, audioContext.currentTime + 0.15); // G3
    }

    oscillator.type = "sine";
    gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
    gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.3);

    oscillator.start(audioContext.currentTime);
    oscillator.stop(audioContext.currentTime + 0.3);
  } catch (e) {
    console.warn("Failed to play notification sound:", e);
  }
}

async function sendOsNotification(payload: NotificationPayload): Promise<void> {
  if (!("Notification" in window)) {
    console.warn("OS notifications not supported");
    return;
  }

  if (Notification.permission === "denied") {
    return;
  }

  if (Notification.permission !== "granted") {
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      return;
    }
  }

  new Notification(payload.title, {
    body: payload.body,
    silent: true, // We handle sound separately
  });
}

function sendToast(payload: NotificationPayload) {
  addToast({
    title: payload.title,
    description: payload.body,
    color: payload.success ? "success" : "danger",
    radius: "sm",
    timeout: 5000,
    shouldShowTimeoutProgress: true,
  });
}

export default function NotificationManager() {
  const executionsRef = useRef<Map<string, BlockExecution>>(new Map());
  const settingsRef = useRef<NotificationSettings | null>(null);

  const refreshSettings = useCallback(async () => {
    settingsRef.current = await loadSettings();
  }, []);

  const shouldNotify = useCallback(
    (durationSecs: number, success: boolean): boolean => {
      const settings = settingsRef.current;
      if (!settings || !settings.enabled) return false;

      // Check focus condition
      if (settings.onlyWhenUnfocused && document.hasFocus()) {
        return false;
      }

      // For failures, check the failure policy
      if (!success) {
        switch (settings.onFailure) {
          case "never":
            return false;
          case "always":
            return true;
          case "after_duration":
            return durationSecs >= settings.minDurationSecs;
        }
      }

      // For success, check duration threshold
      return durationSecs >= settings.minDurationSecs;
    },
    [],
  );

  const notify = useCallback(async (payload: NotificationPayload) => {
    const settings = settingsRef.current;
    if (!settings) return;

    if (settings.osEnabled) {
      sendOsNotification(payload);
    }

    if (settings.toastEnabled) {
      sendToast(payload);
    }

    if (settings.soundEnabled) {
      playNotificationSound(payload.success);
    }
  }, []);

  const handleBlockStarted = useCallback(
    (data: GrandCentralEvents["block-started"]) => {
      executionsRef.current.set(data.block_id, {
        blockId: data.block_id,
        runbookId: data.runbook_id,
        startTime: Date.now(),
      });
    },
    [],
  );

  const handleBlockFinished = useCallback(
    async (data: GrandCentralEvents["block-finished"]) => {
      const execution = executionsRef.current.get(data.block_id);
      executionsRef.current.delete(data.block_id);

      if (!execution) return;

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;

      if (!shouldNotify(durationSecs, data.success)) return;

      // Try to get runbook name for better context
      let runbookName = "Runbook";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore - use default name
      }

      const durationStr = durationSecs >= 60
        ? `${Math.floor(durationSecs / 60)}m ${Math.round(durationSecs % 60)}s`
        : `${durationSecs.toFixed(1)}s`;

      notify({
        title: data.success ? "Block Completed" : "Block Failed",
        body: `${runbookName} - finished in ${durationStr}`,
        success: data.success,
        duration: durationSecs,
      });
    },
    [shouldNotify, notify],
  );

  const handleBlockFailed = useCallback(
    async (data: GrandCentralEvents["block-failed"]) => {
      const execution = executionsRef.current.get(data.block_id);
      executionsRef.current.delete(data.block_id);

      if (!execution) return;

      const durationMs = Date.now() - execution.startTime;
      const durationSecs = durationMs / 1000;

      if (!shouldNotify(durationSecs, false)) return;

      let runbookName = "Runbook";
      try {
        const runbook = await Runbook.load(data.runbook_id);
        if (runbook) {
          runbookName = runbook.name || "Untitled";
        }
      } catch {
        // Ignore
      }

      notify({
        title: "Block Failed",
        body: `${runbookName}: ${data.error}`,
        success: false,
        duration: durationSecs,
      });
    },
    [shouldNotify, notify],
  );

  const handleBlockCancelled = useCallback(
    (data: GrandCentralEvents["block-cancelled"]) => {
      // Just clean up tracking, don't notify on cancellation
      executionsRef.current.delete(data.block_id);
    },
    [],
  );

  useEffect(() => {
    // Load initial settings
    refreshSettings();

    // Refresh settings periodically in case user changes them
    const settingsInterval = setInterval(refreshSettings, 5000);

    // Subscribe to block events
    const unsubStarted = grandCentral.on("block-started", handleBlockStarted);
    const unsubFinished = grandCentral.on("block-finished", handleBlockFinished);
    const unsubFailed = grandCentral.on("block-failed", handleBlockFailed);
    const unsubCancelled = grandCentral.on("block-cancelled", handleBlockCancelled);

    return () => {
      clearInterval(settingsInterval);
      unsubStarted();
      unsubFinished();
      unsubFailed();
      unsubCancelled();
    };
  }, [refreshSettings, handleBlockStarted, handleBlockFinished, handleBlockFailed, handleBlockCancelled]);

  // This component doesn't render anything
  return null;
}
