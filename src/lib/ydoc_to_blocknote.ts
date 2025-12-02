import * as Y from "yjs";
import { Block, BlockNoteEditor } from "@blocknote/core";
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

export async function ydocToBlocknote(doc: Y.Doc): Promise<Block<any>[]> {
  const fragment = doc.getXmlFragment("document-store");
  const editor = getConversionEditor();

  // Direct conversion - no mounting, no waiting, no mutex
  const blocks = yXmlFragmentToBlocks(editor, fragment);

  return blocks;
}
