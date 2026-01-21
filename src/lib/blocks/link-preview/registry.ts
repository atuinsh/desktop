/**
 * Registry for link preview handlers.
 * Allows registering URL matchers that convert pasted URLs into rich preview blocks.
 */

export interface LinkPreviewHandler {
  /** Unique identifier for this handler */
  id: string;

  /** Check if this handler can process the given URL */
  matches: (url: string) => boolean;

  /** Convert the URL into block props. Returns the block type and props. */
  createBlock: (url: string) => { type: string; props: Record<string, string> } | null;
}

class LinkPreviewRegistry {
  private handlers: LinkPreviewHandler[] = [];

  /**
   * Register a new link preview handler.
   * Handlers are checked in order of registration.
   */
  register(handler: LinkPreviewHandler): void {
    // Avoid duplicate registrations
    if (this.handlers.some((h) => h.id === handler.id)) {
      return;
    }
    this.handlers.push(handler);
  }

  /**
   * Find a handler that matches the given URL.
   * Returns the first matching handler, or null if none match.
   */
  findHandler(url: string): LinkPreviewHandler | null {
    for (const handler of this.handlers) {
      if (handler.matches(url)) {
        return handler;
      }
    }
    return null;
  }

  /**
   * Try to create a block from a URL.
   * Returns null if no handler matches or if the handler fails to create a block.
   */
  createBlock(url: string): { type: string; props: Record<string, string> } | null {
    const handler = this.findHandler(url);
    if (!handler) {
      return null;
    }
    return handler.createBlock(url);
  }

  /**
   * Get all registered handlers (for debugging/inspection)
   */
  getHandlers(): readonly LinkPreviewHandler[] {
    return this.handlers;
  }
}

// Singleton instance
export const linkPreviewRegistry = new LinkPreviewRegistry();
