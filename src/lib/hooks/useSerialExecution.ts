import { useEffect, useMemo, useState } from "react";
import {
  onSerialExecutionCancelled,
  onSerialExecutionCompleted,
  onSerialExecutionFailed,
  onSerialExecutionStarted,
} from "../events/grand_central";
import { invoke } from "@tauri-apps/api/core";

export interface SerialExecutionHandle {
  isRunning: boolean;
  isSuccess: boolean;
  isError: boolean;
  isCancelled: boolean;
  error: string | null;
  start: () => void;
  stop: () => void;
}

export function useSerialExecution(runbookId: string) {
  const [isRunning, setIsRunning] = useState(false);
  const [isSuccess, setIsSuccess] = useState(false);
  const [isError, setIsError] = useState(false);
  const [isCancelled, setIsCancelled] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const start = () => {
    invoke("start_serial_execution", { documentId: runbookId });
  };

  const stop = () => {
    invoke("stop_serial_execution", { documentId: runbookId });
  };

  useEffect(() => {
    const unsubs = [];

    unsubs.push(
      onSerialExecutionStarted((data) => {
        if (data.runbook_id === runbookId) {
          setIsRunning(true);
          setIsSuccess(false);
          setIsError(false);
          setIsCancelled(false);
          setError(null);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionCompleted((data) => {
        console.log("onSerialExecutionCompleted", data);
        if (data.runbook_id === runbookId) {
          setIsRunning(false);
          setIsSuccess(true);
          setIsError(false);
          setIsCancelled(false);
          setError(null);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionCancelled((data) => {
        console.log("onSerialExecutionCancelled", data);
        if (data.runbook_id === runbookId) {
          setIsSuccess(false);
          setIsError(false);
          setIsCancelled(true);
          setError(null);
          setIsRunning(false);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionFailed((data) => {
        console.log("onSerialExecutionFailed", data);
        if (data.runbook_id === runbookId) {
          setIsSuccess(false);
          setIsError(true);
          setIsCancelled(false);
          setError(data.error);
          setIsRunning(false);
        }
      }),
    );
  }, []);

  const handle = useMemo(
    () => ({
      isRunning: isRunning,
      isSuccess: isSuccess,
      isError: isError,
      isCancelled: isCancelled,
      error: error,
      start: start,
      stop: stop,
    }),
    [isRunning, start, stop],
  );

  return handle;
}
