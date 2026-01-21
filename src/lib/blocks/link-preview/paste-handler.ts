import { linkPreviewRegistry } from "./registry";

interface PasteHandlerContext {
  event: ClipboardEvent;
  editor: any; // BlockNoteEditor
  defaultPasteHandler: (context?: {
    prioritizeMarkdownOverHTML?: boolean;
    plainTextAsMarkdown?: boolean;
  }) => boolean | undefined;
}

/**
 * Normalize pasted text by stripping common URL wrappers.
 */
function normalizeUrl(text: string): string {
  let url = text.trim();

  // Strip angle brackets: <https://...>
  if (url.startsWith("<") && url.endsWith(">")) {
    url = url.slice(1, -1);
  }

  return url;
}

/**
 * Generic paste handler for link previews.
 * Checks the link preview registry for handlers that match the pasted URL.
 *
 * - On empty paragraph + matching URL → converts to preview block
 * - Otherwise → falls through to default paste handler
 */
export function linkPreviewPasteHandler({
  event,
  editor,
  defaultPasteHandler,
}: PasteHandlerContext): boolean | undefined {
  const rawText = event.clipboardData?.getData("text/plain");
  if (!rawText) {
    return defaultPasteHandler();
  }

  const url = normalizeUrl(rawText);

  // Check if any handler matches this URL
  const handler = linkPreviewRegistry.findHandler(url);
  if (!handler) {
    return defaultPasteHandler();
  }

  // Check if current block is empty
  const currentBlock = editor.getTextCursorPosition()?.block;
  if (!currentBlock) {
    return defaultPasteHandler();
  }

  // Only convert on blank line - check if block is empty paragraph
  const isEmptyParagraph =
    currentBlock.type === "paragraph" &&
    (!currentBlock.content || currentBlock.content.length === 0);

  if (!isEmptyParagraph) {
    return defaultPasteHandler();
  }

  // Create the block
  const block = handler.createBlock(url);
  if (!block) {
    return defaultPasteHandler();
  }

  // Replace the empty paragraph with the preview block
  editor.updateBlock(currentBlock, {
    type: block.type,
    props: block.props,
  });

  return true;
}
