import { Alert, Button, PressEvent } from "@heroui/react";
import { InfoIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { useKvValue } from "@/lib/hooks/useKvValue";
import { useState } from "react";

interface RuntimeUpdateNoticeProps {
  openRunbookImport: () => void;
}

export default function RuntimeUpdateNotice(props: RuntimeUpdateNoticeProps) {
  const [temporarilyHideAlert, setTemporarilyHideAlert] = useState(false);

  // Default to true to prevent the alert from showing on first launch,
  // but if the value isn't found, set it to false so the alert shows.
  const [dismissedNewRuntimeAlert, setDismissedNewRuntimeAlert] = useKvValue<boolean>(
    "dismissed_new_runtime_alert",
    true,
    Some(false),
  );

  function handleRuntimeExpainerRunbookClick(_e: PressEvent) {
    props.openRunbookImport();
  }

  function closeAlert() {
    setDismissedNewRuntimeAlert(true);
  }

  if (dismissedNewRuntimeAlert === true || temporarilyHideAlert === true) {
    return null;
  }

  return (
    <div
      // z-index for blocknote hover indicators is 1000
      className="absolute bordered z-[1100] w-[500px] max-w-[50%]"
      style={{ bottom: 20, left: 95 }}
    >
      <Alert
        color="success"
        icon={<InfoIcon />}
        isClosable={true}
        onClose={closeAlert}
        title="Atuin Desktop's New Execution Engine"
        classNames={{
          base: cn(["!bg-success-50/100"]),
          title: cn(["!text-lg", "!font-semibold"]),
        }}
        description={
          <>
            <div className="mb-2">
              This version of Atuin Desktop ships with a{" "}
              <strong>brand new runbook execution engine</strong>. For more information on the new
              runtime and any changes you should be aware of, please see the runtime runbook, linked
              below:
            </div>
            <Button
              onPress={handleRuntimeExpainerRunbookClick}
              type="button"
              role="link"
              color="success"
            >
              Open Runtime Runbook
            </Button>
            <Button
              onPress={() => setTemporarilyHideAlert(true)}
              type="button"
              role="link"
              variant="ghost"
              className="ml-2"
            >
              Remind me Later
            </Button>
          </>
        }
      ></Alert>
    </div>
  );
}
