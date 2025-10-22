import { me, HttpResponseError } from "@/api/api";
import Runbook from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";

const handleDeepLink = async (
  url: string,
  openRunbook: (id: string) => void,
): Promise<void | null> => {
  const uri = URL.parse(url);
  if (!uri) {
    return null;
  }

  if (uri.host === "runbook") {
    const runbookId = uri.pathname.substring(1); // drop leading slash
    const tag = uri.searchParams.get("tag") || "latest";

    const runbook = await Runbook.load(runbookId);
    if (runbook) {
      openRunbook(runbook.id);
      return;
    } else {
      // If we're in the middle of importing, don't show the dialog
      if (useStore.getState().openInDesktopImport) {
        return;
      }
      useStore.getState().setOpenInDesktopImport({ id: runbookId, tag });
    }
  } else if (uri.host === "register-token") {
    const token = uri.pathname.substring(1); // drop leading slash

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
  }
};

export default handleDeepLink;
