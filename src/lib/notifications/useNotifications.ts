import { useCallback } from "react";
import { addToast } from "@heroui/react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export interface NotifyOptions {
  title: string;
  body: string;
  success?: boolean;
  // Explicitly enable notification channels (all default to false)
  os?: boolean;
  toast?: boolean;
  sound?: boolean;
}

function playSound(success: boolean) {
  try {
    const audioContext = new AudioContext();
    const oscillator = audioContext.createOscillator();
    const gainNode = audioContext.createGain();

    oscillator.connect(gainNode);
    gainNode.connect(audioContext.destination);

    if (success) {
      oscillator.frequency.setValueAtTime(523.25, audioContext.currentTime);
      oscillator.frequency.setValueAtTime(659.25, audioContext.currentTime + 0.1);
    } else {
      oscillator.frequency.setValueAtTime(220, audioContext.currentTime);
      oscillator.frequency.setValueAtTime(196, audioContext.currentTime + 0.15);
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

async function sendOsNotification(title: string, body: string): Promise<void> {
  try {
    let hasPermission = await isPermissionGranted();

    if (!hasPermission) {
      const permission = await requestPermission();
      hasPermission = permission === "granted";
    }

    if (!hasPermission) return;

    sendNotification({ title, body });
  } catch (e) {
    console.warn("Failed to send OS notification:", e);
  }
}

function sendToast(title: string, body: string, success: boolean) {
  addToast({
    title,
    description: body,
    color: success ? "success" : "danger",
    radius: "sm",
    timeout: 5000,
    shouldShowTimeoutProgress: true,
  });
}

/**
 * Hook for sending manual/imperative notifications.
 * This is separate from the automatic NotificationManager which handles
 * block and serial execution events.
 */
export function useNotifications() {
  const notify = useCallback(async (options: NotifyOptions) => {
    const { title, body, success = true, os = false, toast = false, sound = false } = options;

    if (os) {
      sendOsNotification(title, body);
    }

    if (toast) {
      sendToast(title, body, success);
    }

    if (sound) {
      playSound(success);
    }
  }, []);

  return { notify };
}
