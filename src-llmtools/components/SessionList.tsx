import type { SessionInfo } from "../types";

interface SessionListProps {
  sessions: SessionInfo[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export default function SessionList({ sessions, selectedId, onSelect }: SessionListProps) {
  if (sessions.length === 0) {
    return (
      <div className="p-4 text-sm text-default-400">
        No active sessions
      </div>
    );
  }

  return (
    <div className="p-2">
      {sessions.map((session) => (
        <button
          key={session.id}
          onClick={() => onSelect(session.id)}
          className={`
            w-full text-left p-3 rounded-lg mb-1 transition-colors
            ${selectedId === session.id
              ? "bg-primary-100 dark:bg-primary-900/30"
              : "hover:bg-default-100 dark:hover:bg-default-800"
            }
          `}
        >
          <div className="text-sm font-medium truncate">
            {session.kind === "assistantChat" ? "Assistant Chat" : "Inline Generation"}
          </div>
          <div className="text-xs text-default-400 truncate mt-1">
            {session.id.slice(0, 8)}...
          </div>
          <div className="text-xs text-default-500 truncate mt-0.5">
            Runbook: {session.runbookId.slice(0, 8)}...
          </div>
        </button>
      ))}
    </div>
  );
}
