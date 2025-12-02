import * as Y from "yjs";
import { BlockNoteEditor } from "@blocknote/core";
import { yXmlFragmentToBlocks } from "@blocknote/core/yjs";
import { schema } from "@/components/runbooks/editor/create_editor";

// Create a single editor instance for conversions - reusable, no DOM needed
let conversionEditor: BlockNoteEditor<typeof schema.blockSchema, typeof schema.inlineContentSchema, typeof schema.styleSchema> | null = null;

function getConversionEditor() {
  if (!conversionEditor) {
    conversionEditor = BlockNoteEditor.create({ schema });
  }
  return conversionEditor;
}

// Synchronous conversion from Y.Doc to BlockNote blocks
export function ydocToBlocknote(doc: Y.Doc): any[] {
  const fragment = doc.getXmlFragment("document-store");
  const editor = getConversionEditor();

  // Direct conversion - no mounting, no waiting, no mutex
  const blocks = yXmlFragmentToBlocks(editor, fragment);

  return blocks;
}

// Convert raw ydoc bytes to BlockNote blocks
export function ydocBytesToBlocknote(ydocBytes: Uint8Array): any[] {
  const doc = new Y.Doc();
  Y.applyUpdate(doc, ydocBytes);
  return ydocToBlocknote(doc);
}
