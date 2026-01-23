import { useEffect, useState, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import type { SessionInfo, LLMToolsEvent, SessionEvent } from "./types";
import SessionList from "./components/SessionList";
import SessionDetail from "./components/SessionDetail";

type SessionEventWithId = {
  sessionId: string;
  event: SessionEvent;
};

export default function App() {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [eventsBySession, setEventsBySession] = useState<Map<string, SessionEventWithId[]>>(new Map());
  const [connected, setConnected] = useState(false);

  const handleEvent = useCallback((event: LLMToolsEvent) => {
    if (event.type === "sessionCreated") {
      setSessions((prev) => [...prev, event.info]);
    } else if (event.type === "sessionDestroyed") {
      setSessions((prev) => prev.filter((s) => s.id !== event.session_id));
      setEventsBySession((prev) => {
        const next = new Map(prev);
        next.delete(event.session_id);
        return next;
      });
    } else if (event.type === "sessionEvent") {
      setEventsBySession((prev) => {
        const next = new Map(prev);
        const existing = next.get(event.session_id) || [];
        next.set(event.session_id, [
          ...existing,
          { sessionId: event.session_id, event: event.event },
        ]);
        return next;
      });
    }
  }, []);

  useEffect(() => {
    // Fetch initial session list
    invoke<SessionInfo[]>("llmtools_list_sessions")
      .then((sessionList) => {
        setSessions(sessionList);
      })
      .catch((err) => {
        console.error("Failed to list sessions:", err);
      });

    // Subscribe to events
    const channel = new Channel<LLMToolsEvent>();
    channel.onmessage = handleEvent;

    invoke("llmtools_subscribe", { channel })
      .then(() => {
        setConnected(true);
      })
      .catch((err) => {
        console.error("Failed to subscribe to LLM Tools events:", err);
      });

    // Cleanup not needed - channel will be closed when component unmounts
  }, [handleEvent]);

  const selectedSession = sessions.find((s) => s.id === selectedSessionId);
  const selectedEvents = selectedSessionId
    ? eventsBySession.get(selectedSessionId) || []
    : [];

  return (
    <div className="h-full flex flex-col bg-background text-foreground">
      {/* Header */}
      <div className="flex-none p-4 border-b border-divider">
        <h1 className="text-lg font-semibold">LLM Tools</h1>
        <p className="text-sm text-default-500">
          {connected ? `${sessions.length} active session(s)` : "Connecting..."}
        </p>
      </div>

      {/* Main content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Session list sidebar */}
        <div className="w-64 flex-none border-r border-divider overflow-y-auto">
          <SessionList
            sessions={sessions}
            selectedId={selectedSessionId}
            onSelect={setSelectedSessionId}
          />
        </div>

        {/* Session detail area */}
        <div className="flex-1 overflow-hidden">
          {selectedSession ? (
            <SessionDetail session={selectedSession} events={selectedEvents} />
          ) : (
            <div className="h-full flex items-center justify-center text-default-400">
              <p>Select a session to view details</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
