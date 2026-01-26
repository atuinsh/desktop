import { useCallback } from "react";
import { AIPopupBase } from "./ui/AIPopupBase";
import track_event from "@/tracking";

interface AIGeneratePopupProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onSubmit: (prompt: string) => void;
  onClose: () => void;
}

/**
 * Popup for collecting a prompt to generate blocks.
 * The actual generation is handled by the parent via onSubmit callback.
 */
export function AIGeneratePopup({
  isVisible,
  position,
  onSubmit,
  onClose,
}: AIGeneratePopupProps) {
  const handleSubmit = useCallback(
    async (prompt: string) => {
      track_event("runbooks.ai.generate_popup", { prompt_length: prompt.length });
      onSubmit(prompt);
      // Close immediately - generation UI is handled by the inline generation hook
    },
    [onSubmit],
  );

  return (
    <AIPopupBase
      isVisible={isVisible}
      position={position}
      onClose={onClose}
      onSubmit={handleSubmit}
      title="Generate block"
      placeholder="e.g., curl command to fetch users, SQL query to find recent orders..."
      submitButtonText="Generate"
      submitButtonLoadingText="Generating..."
      showSuggestions={false}
    />
  );
}
