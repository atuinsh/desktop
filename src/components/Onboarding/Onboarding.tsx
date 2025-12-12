import {
  Modal,
  ModalContent,
  Button,
  Switch,
  useDisclosure,
  Card,
  CardBody,
  CardHeader,
  Divider,
} from "@heroui/react";

import { useEffect, useState } from "react";
import { Icon } from "@iconify/react";
import { KVStore } from "@/state/kv";
import AccountModal from "../Account/AccountModal";
import { init_tracking } from "@/tracking";

const FeatureCard = ({ title, description }: any) => (
  <Card>
    <CardBody>
      <h4 className="text-lg font-semibold mb-2">{title}</h4>
      <p className="text-sm text-gray-600">{description}</p>
    </CardBody>
  </Card>
);

const Onboarding = () => {
  let {
    isOpen: isOnboardingOpen,
    onOpen: onOnboardingOpen,
    onOpenChange: onOnboardingOpenChange,
  } = useDisclosure();

  const [trackingOptIn, setTrackingOptIn] = useState(true);
  const [showAccountModal, setShowAccountModal] = useState(false);

  useEffect(() => {
    // On first mount, if there is no tracking in the kv, we should set it to true in order to match the above default.
    (async () => {
      let db = await KVStore.open_default();
      let track = await db.get<boolean>("usage_tracking");
      if (track === null) {
        await db.set("usage_tracking", true);
      }
    })();

    onOnboardingOpen();
  }, []);

  const close = async (onClose: any) => {
    onClose();

    setShowAccountModal(true);
  };

  return (
    <>
      <Modal
        disableAnimation
        isDismissable={false}
        hideCloseButton
        isOpen={isOnboardingOpen}
        onOpenChange={onOnboardingOpenChange}
        className="w-full select-none"
        size="2xl"
        onClose={async () => {
          await init_tracking();
        }}
      >
        <ModalContent className="w-full">
          {(onClose) => (
            <div className="max-w-[900px] mx-auto p-6 space-y-6">
              <h1 className="text-4xl font-bold text-center">Welcome to Atuin Desktop</h1>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <FeatureCard
                  title="Runbooks that Run"
                  description="Create and run notebooks that integrate seamlessly with your infrastructure"
                />
                <FeatureCard
                  title="Shell History Explorer"
                  description="Easily search and analyze your past commands"
                />
              </div>

              <Card>
                <CardHeader className="flex gap-3">
                  <div className="flex flex-col">
                    <p className="text-md">Getting Started</p>
                  </div>
                </CardHeader>
                <Divider />
                <CardBody>
                  <ul className="list-disc pl-6 space-y-2">
                    <li>Select a runbook from the sidebar to get started</li>
                    <li>
                      Use the <Icon className="inline-block" icon="solar:history-outline" /> icon to
                      explore your shell history
                    </li>
                    <li>
                      Join the{" "}
                      <a
                        href="https://dub.sh/atuin-desktop-beta"
                        target="_blank"
                        className="text-blue-400 underline"
                      >
                        community
                      </a>{" "}
                      to get help and share feedback
                    </li>
                    <li>
                      Read the docs at{" "}
                      <a
                        href="https://docs.atuin.sh/desktop"
                        target="_blank"
                        className="text-blue-400 underline"
                      >
                        docs.atuin.sh/desktop
                      </a>
                    </li>
                  </ul>
                </CardBody>
              </Card>

              <Card>
                <CardBody className="gap-4">
                  <h2 className="text-xl font-bold">Usage Tracking</h2>
                  <p className="text-gray-600">
                    To help improve Atuin, we'd like to collect anonymous usage data and error
                    reports. We respect your privacy and only track with your permission.
                  </p>
                  <div className="flex items-center justify-between">
                    <p className="font-semibold">Enable tracking</p>
                    <Switch
                      isSelected={trackingOptIn}
                      onValueChange={(value) => {
                        (async () => {
                          let db = await KVStore.open_default();
                          await db.set("usage_tracking", value);
                        })();

                        setTrackingOptIn(value);
                      }}
                      aria-label="Toggle tracking"
                    />
                  </div>
                  {trackingOptIn && (
                    <p className="text-sm text-gray-500">
                      Thank you for helping us improve Atuin. You can change this setting anytime.
                    </p>
                  )}
                  {!trackingOptIn && (
                    <p className="text-sm text-gray-500">
                      Tracking is disabled. No data will be collected.
                    </p>
                  )}
                </CardBody>
              </Card>

              <Button color="success" className="w-full" onClick={() => close(onClose)}>
                Next
              </Button>
            </div>
          )}
        </ModalContent>
      </Modal>
      <AccountModal close={() => setShowAccountModal(false)} isOpen={showAccountModal} />
    </>
  );
};

export default Onboarding;
