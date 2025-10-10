import {
  Button,
  Input,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/react";
import { useState } from "react";

interface SaveBlockModalProps {
  block: any;
  onClose: () => void;
  doSaveBlock: (name: string, block: any) => void;
}

export default function SaveBlockModal(props: SaveBlockModalProps) {
  const [blockName, setBlockName] = useState("");

  function handleClose() {
    props.onClose();
  }

  function confirmSaveBlock() {
    props.doSaveBlock(blockName, props.block);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      confirmSaveBlock();
    } else if (e.key === "Escape") {
      handleClose();
    }
  }

  return (
    <Modal isOpen onClose={handleClose}>
      <ModalContent>
        {(onClose) => (
          <>
            <ModalHeader>Save Block</ModalHeader>
            <ModalBody>
              <p>Save this block so you can quickly insert it into runbooks later.</p>
              <Input
                autoFocus
                placeholder="Block Name"
                value={blockName}
                onValueChange={setBlockName}
                onKeyDown={handleKeyDown}
              />
            </ModalBody>
            <ModalFooter>
              <Button onPress={onClose}>Cancel</Button>
              <Button onPress={confirmSaveBlock} color="primary">
                Save
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  );
}
