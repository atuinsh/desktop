import { inviteFriends } from "@/api/user";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import {
  addToast,
  Alert,
  Button,
  Chip,
  Input,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/react";
import { useState } from "react";

type Invite = {
  email: string;
};

const BASIC_EMAIL_REGEX = /^[^\s@,]+@[^\s@,]+\.[^\s@,]+$/;

function isValidEmail(email: string) {
  if (email === "") return false;
  if (BASIC_EMAIL_REGEX.test(email)) return true;

  const possibleSplit = email
    .split(/[\s,]+/)
    .map((e) => e.trim())
    .filter((e) => e !== "");
  return possibleSplit.every((e) => BASIC_EMAIL_REGEX.test(e));
}

type InviteFriendsModalProps = {
  isOpen: boolean;
  onClose: () => void;
};

export default function InviteFriendsModal(props: InviteFriendsModalProps) {
  const connectionState = useStore((state) => state.connectionState);
  const [emails, setEmails] = useState<Invite[]>([]);
  const [newEmailValid, setNewEmailValid] = useState<boolean>(false);
  const [newEmail, setNewEmail] = useState<string>("");

  function closeAndReset() {
    props.onClose();
    setEmails([]);
    setNewEmail("");
    setNewEmailValid(false);
  }

  function handleNewEmailChange(e: React.ChangeEvent<HTMLInputElement>) {
    const email = e.target.value;
    setNewEmail(email);
    setNewEmailValid(isValidEmail(email));
  }

  function handleAddEmail() {
    const email = newEmail.trim();
    const newEmails = email
      .split(/[\s,]+/)
      .map((e) => e.trim())
      .filter((e) => {
        if (e === "") return false;
        if (emails.some((e2) => e2.email === e)) return false;
        if (!BASIC_EMAIL_REGEX.test(e)) return false;
        return true;
      });

    setEmails((emails) => [...emails, ...newEmails.map((e) => ({ email: e }))]);
    setNewEmail("");
  }

  function handleRemoveEmail(email: Invite) {
    setEmails((emails) => emails.filter((e) => e.email !== email.email));
  }

  function handleFormSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    handleAddEmail();
  }

  async function handleSend() {
    if (connectionState !== ConnectionState.Online) {
      addToast({
        title: "Error",
        description: "Please check your internet connection and try again.",
        color: "danger",
      });
      return;
    }

    const emailsToSend = emails.map((e) => e.email);
    try {
      const result = await inviteFriends(emailsToSend);
      console.log(result);
      addToast({
        title: "Success",
        description: "Invitations sent successfully.",
        color: "success",
        shouldShowTimeoutProgress: true,
      });
      closeAndReset();
    } catch (error) {
      addToast({
        title: "Error",
        description: "We weren't able to send the invitations. Please try again later.",
        color: "danger",
        shouldShowTimeoutProgress: true,
      });
    }
  }

  return (
    <Modal isOpen={props.isOpen} onClose={closeAndReset}>
      <ModalContent>
        <ModalHeader>Invite Friends</ModalHeader>
        <ModalBody>
          <p>Invite friends and colleagues to try Atuin Desktop!</p>
          {connectionState !== ConnectionState.Online && (
            <Alert color="danger" className="my-4">
              You don't appear to be online. Please check your internet connection.
            </Alert>
          )}
          <form
            className="flex flex-row gap-2 items-center"
            onSubmit={handleFormSubmit}
          >
            <Input
              label="Add invite"
              placeholder="email@domain.com"
              value={newEmail}
              onChange={handleNewEmailChange}
            />
            <Button onPress={handleAddEmail} isDisabled={!newEmailValid}>
              Add
            </Button>
          </form>
          <div>
            {emails.map((email, index) => (
              <Chip key={index} className="m-1" onClose={() => handleRemoveEmail(email)}>
                {email.email}
              </Chip>
            ))}
          </div>
        </ModalBody>
        <ModalFooter>
          <Button onPress={closeAndReset} variant="flat">
            Cancel
          </Button>
          <Button
            onPress={handleSend}
            color="primary"
            isDisabled={emails.length === 0 || connectionState !== ConnectionState.Online}
          >
            {emails.length > 0
              ? `Send ${emails.length} ${emails.length === 1 ? "Invite" : "Invites"}`
              : "Add emails to invite"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
