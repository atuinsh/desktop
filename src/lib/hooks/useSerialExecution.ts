import { useCallback, useEffect, useMemo, useState } from "react";
import {
  onSerialExecutionCancelled,
  onSerialExecutionCompleted,
  onSerialExecutionFailed,
  onSerialExecutionPaused,
  onSerialExecutionStarted,
} from "../events/grand_central";
import { invoke } from "@tauri-apps/api/core";
import { UnsubscribeFunction } from "emittery";

export interface SerialExecutionHandle {
  isRunning: boolean;
  isSuccess: boolean;
  isError: boolean;
  isCancelled: boolean;
  isPaused: boolean;
  pausedAtBlockId: string | null;
  error: string | null;
  start: () => void;
  stop: () => void;
  resumeFrom: (blockId: string) => void;
}

export function useSerialExecution(runbookId: string | undefined | null) {
  const [isRunning, setIsRunning] = useState(false);
  const [isSuccess, setIsSuccess] = useState(false);
  const [isError, setIsError] = useState(false);
  const [isCancelled, setIsCancelled] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [pausedAtBlockId, setPausedAtBlockId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const start = useCallback(() => {
    if (!runbookId) return;
    invoke("start_serial_execution", { documentId: runbookId });
  }, [runbookId]);

  const stop = useCallback(() => {
    if (!runbookId) return;
    invoke("stop_serial_execution", { documentId: runbookId });
  }, [runbookId]);

  const resumeFrom = useCallback(
    (blockId: string) => {
      if (!runbookId) return;
      invoke("start_serial_execution", {
        documentId: runbookId,
        fromBlock: blockId,
      });
    },
    [runbookId],
  );

  useEffect(() => {
    const unsubs: UnsubscribeFunction[] = [];

    unsubs.push(
      onSerialExecutionStarted((data) => {
        if (data.runbook_id === runbookId) {
          setIsRunning(true);
          setIsSuccess(false);
          setIsError(false);
          setIsCancelled(false);
          setIsPaused(false);
          setPausedAtBlockId(null);
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
          setIsPaused(false);
          setPausedAtBlockId(null);
          setError(null);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionCancelled((data) => {
        console.log("onSerialExecutionCancelled", data);
        if (data.runbook_id === runbookId) {
          setIsRunning(false);
          setIsSuccess(false);
          setIsError(false);
          setIsCancelled(true);
          setIsPaused(false);
          setPausedAtBlockId(null);
          setError(null);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionFailed((data) => {
        console.log("onSerialExecutionFailed", data);
        if (data.runbook_id === runbookId) {
          setIsRunning(false);
          setIsSuccess(false);
          setIsError(true);
          setIsCancelled(false);
          setIsPaused(false);
          setPausedAtBlockId(null);
          setError(data.error);
        }
      }),
    );

    unsubs.push(
      onSerialExecutionPaused((data) => {
        console.log("onSerialExecutionPaused", data);
        if (data.runbook_id === runbookId) {
          setIsRunning(false);
          setIsSuccess(false);
          setIsError(false);
          setIsCancelled(false);
          setIsPaused(true);
          setPausedAtBlockId(data.block_id);
          setError(null);
        }
      }),
    );

    return () => {
      unsubs.forEach((unsub) => unsub());
    };
  }, [runbookId]);

  const handle = useMemo(
    () => ({
      isRunning,
      isSuccess,
      isError,
      isCancelled,
      isPaused,
      pausedAtBlockId,
      error,
      start,
      stop,
      resumeFrom,
    }),
    [
      isRunning,
      isSuccess,
      isError,
      isCancelled,
      isPaused,
      pausedAtBlockId,
      error,
      start,
      stop,
      resumeFrom,
    ],
  );

  return handle;
}
