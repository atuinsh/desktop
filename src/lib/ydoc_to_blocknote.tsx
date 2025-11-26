import * as Y from "yjs";
import debounce from "lodash.debounce";
import { createConversionEditor } from "@/components/runbooks/editor/create_editor";
import { Block } from "@blocknote/core";
import Mutex from "./std/mutex";

let convertMutex = new Mutex();

export async function ydocToBlocknote(doc: Y.Doc): Promise<Block<any>[]> {
  const unlock = await convertMutex.lock();

  const promise = new Promise<Block<any>[]>((resolve, reject) => {
    let resolved = false;

    const fragment = doc.getXmlFragment("document-store");
    const editor = createConversionEditor(doc, fragment);

    editor.onChange(
      debounce((editor) => {
        if (resolved) return;
        resolved = true;
        resolve(editor.document);
        editor.unmount();
      }, 100),
    );

    const el = document.createElement("div");
    setTimeout(() => {
      if (resolved) return;
      resolved = true;
      reject(new Error("Yjs to BlockNote conversation timed out"));
      editor.unmount();
    }, 5000);

    editor.mount(el);
  });

  promise.finally(() => unlock());
  return promise;
}
