import { savedBlocks } from "@/lib/queries/saved_blocks";
import {
  Button,
  Input,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
} from "@heroui/react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";

interface SaveBlockModalProps {
  block: any;
  onClose: () => void;
  doSaveBlock: (name: string, block: any) => void;
}

export default function SaveBlockModal(props: SaveBlockModalProps) {
  const [blockName, setBlockName] = useState("");
  const { data: fetchedSavedBlocks } = useQuery(savedBlocks());

  const hasNameConflict = useMemo(() => {
    return fetchedSavedBlocks?.some((savedBlock) => savedBlock.get("name") === blockName);
  }, [fetchedSavedBlocks, blockName]);

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
                isInvalid={hasNameConflict}
                errorMessage="A saved block with this name already exists. Saving with this name will overwrite the existing block."
              />
            </ModalBody>
            <ModalFooter>
              <Button onPress={onClose}>Cancel</Button>
              <Button
                onPress={confirmSaveBlock}
                color={hasNameConflict ? "danger" : "primary"}
                isDisabled={blockName.trim().length === 0}
              >
                {hasNameConflict ? "Overwrite" : "Save"}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  );
}
