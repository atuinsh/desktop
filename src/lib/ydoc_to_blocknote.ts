import * as Y from "yjs";
import { Block, BlockNoteEditor } from "@blocknote/core";
import { yXmlFragmentToBlocks } from "@blocknote/core/yjs";
import { schema } from "@/components/runbooks/editor/create_editor";

// Synchronous conversion from Y.Doc to BlockNote blocks
export function ydocToBlocknote(doc: Y.Doc): any[] {
  console.time("ydocToBlocknote");
  const fragment = doc.getXmlFragment("document-store");

  // Create a fresh editor for each conversion to avoid race conditions
  const editor = BlockNoteEditor.create({ schema });
  const blocks = yXmlFragmentToBlocks(editor, fragment);

  console.timeEnd("ydocToBlocknote");

  return blocks;
}

// Convert raw ydoc bytes to BlockNote blocks
export function ydocBytesToBlocknote(ydocBytes: Uint8Array): Block<any>[] {
  const doc = new Y.Doc();
  Y.applyUpdate(doc, ydocBytes);
  return ydocToBlocknote(doc);
}
