import * as Y from "yjs";
import { createConversionEditor } from "@/components/runbooks/editor/create_editor";
import { Block } from "@blocknote/core";
import Mutex from "./std/mutex";
import { yDocToBlocks } from "@blocknote/core/yjs";

let convertMutex = new Mutex();

export async function ydocToBlocknote(doc: Y.Doc): Promise<Block<any>[]> {
  const unlock = await convertMutex.lock();

  const editor = createConversionEditor(doc, doc.getXmlFragment("document-store"));
  const blocks = yDocToBlocks(editor, doc, "document-store");

  unlock();
  return blocks as Block<any>[];
}
