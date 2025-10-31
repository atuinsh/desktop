import track_event from "@/tracking";
import { Button, Tooltip } from "@heroui/react";
import { Play, RefreshCw, Square } from "lucide-react";
import { useEffect, useState } from "react";

interface PlayButtonProps {
  isRunning: boolean;
  isLoading?: boolean;
  cancellable: boolean;

  /// Called when the play button is pressed, and when stop is pressed if onStop is not specified.
  onPlay: () => void;

  /// If specified, call this when stop is pressed. Otherwise onPlay is called again.
  onStop?: () => void;
  onRefresh?: () => void;

  /// If specified, this will be the event name that is tracked when the button is clicked.
  eventName?: string;
  /// Additional properties to pass to the tracking event.
  eventProps?: Record<string, any>;
  disabled?: boolean;

  // If specified, the button will not be disabled when in running state.
  alwaysStop?: boolean;
  className?: string;

  tooltip?: string;
  tooltipPlacement?: "top" | "bottom" | "left" | "right";
}

const PlayButton = ({
  isRunning,
  isLoading = false,
  onPlay,
  cancellable,
  eventName,
  eventProps,
  onStop,
  onRefresh,
  disabled,
  alwaysStop,
  className,
  tooltip,
  tooltipPlacement,
}: PlayButtonProps) => {
  const [isShiftPressed, setIsShiftPressed] = useState(false);

  // bind shift
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Shift") setIsShiftPressed(true);
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Shift") setIsShiftPressed(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, []);

  const renderButton = () => {
    return (
      <Button
        isIconOnly
        color={isRunning ? (isShiftPressed && onRefresh ? "warning" : "danger") : "success"}
        variant="flat"
        size="sm"
        aria-label={isRunning ? "Stop code" : "Run code"}
        isDisabled={disabled && (!alwaysStop || !isRunning)}
        onPress={async () => {
          // If we're not running, call onPlay.
          // If we are running, but have not specified onStop, call onPlay again.
          // If we are running and have specified onStop, call onStop.
          if (eventName) {
            track_event(eventName, eventProps || {});
          }

          if (isRunning && isShiftPressed && onRefresh) {
            onRefresh();
            return;
          }

          if (isRunning && onStop) {
            onStop();
          } else {
            onPlay();
          }
        }}
        className={`w-8 h-8 min-w-unit-8 min-h-unit-8 ${className}`}
        isLoading={(isRunning && !cancellable) || isLoading}
      >
        <span
          className={`transition-transform duration-300 ease-in-out ${
            isRunning ? "rotate-180" : ""
          }`}
        >
          {isRunning ? (
            isShiftPressed && onRefresh ? (
              <RefreshCw size={16} />
            ) : (
              <Square size={16} />
            )
          ) : (
            <Play size={16} />
          )}
        </span>
      </Button>
    );
  };

  if (tooltip) {
    return (
      <Tooltip content={tooltip} placement={tooltipPlacement || "bottom"}>
        {renderButton()}
      </Tooltip>
    );
  } else if (onRefresh && isRunning) {
    return (
      <Tooltip content="Hold shift to re-run" delay={500}>
        {renderButton()}
      </Tooltip>
    );
  }

  return renderButton();
};

export default PlayButton;
