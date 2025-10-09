import { me, HttpResponseError } from "@/api/api";
import { DialogBuilder } from "@/components/Dialogs/dialog";
import Runbook from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";

interface RouteHandler {
  (params: string[]): void;
}

interface Routes {
  [pattern: string]: RouteHandler;
}

const handleDeepLink = (url: string, openRunbook: (id: string) => void): void | null => {
  const routes: Routes = {
    // New "Open in desktop" from the hub
    "^atuin://runbook/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$": async (
      params: string[],
    ) => {
      const [id] = params;

      const runbook = await Runbook.load(id);
      if (runbook) {
        openRunbook(runbook.id);
      } else {
        await new DialogBuilder()
          .title("Runbook not found")
          .message("The runbook you are trying to open was not found on this machine.")
          .action({ label: "OK", value: "ok", variant: "flat" })
          .build();
      }
    },

    // Register an auth token with this desktop app
    "^atuin://register-token/(atapi_[0-9A-F]+)$": async (params: string[]) => {
      const [token] = params;

      try {
        // Hit the "me" endpoint to verify the token and fetch username
        let user = await me(token);

        useStore.getState().setProposedDesktopConnectuser({ username: user.user.username, token });
      } catch (err) {
        if (err instanceof HttpResponseError) {
          console.error("Failed to verify token:", err.code);
        } else {
          console.error("Failed to verify token:", err);
        }
      }
    },
  };

  // If no URL provided, return early
  if (!url) {
    return null;
  }

  // Try to match the URL against each route pattern
  for (const [pattern, handler] of Object.entries(routes)) {
    const regex = new RegExp(pattern, "i");
    const match = url.match(regex);

    if (match) {
      // Extract the captured groups (excluding the full match)
      const params = match.slice(1);

      try {
        return handler(params);
      } catch (error) {
        console.error(`Error handling route for URL ${url}:`, error);
        return null;
      }
    }
  }

  // No matching route found
  console.warn(`No matching route found for URL: ${url}`);
  return null;
};

export default handleDeepLink;
