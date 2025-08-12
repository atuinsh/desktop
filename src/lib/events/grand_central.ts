import Emittery from 'emittery';
import { invoke, Channel } from '@tauri-apps/api/core';
import type { GCEvent } from '@/rs-bindings/GCEvent';

// Event map for type-safe event handling
export interface GrandCentralEvents {
  'pty-opened': { pty_id: string; runbook: string; block: string; created_at: number };
  'pty-closed': { pty_id: string };
}

/**
 * Grand Central Event System
 * 
 * Provides a type-safe, centralized event system for the entire application.
 * Extends Emittery to provide additional functionality for subscribing to
 * backend events via Tauri channels.
 */
export class GrandCentral extends Emittery<GrandCentralEvents> {
  private isListening = false;
  private unsubscribe?: () => void;

  constructor() {
    super();
  }

  /**
   * Start listening to events from the backend
   */
  async startListening(): Promise<void> {
    if (this.isListening) {
      return;
    }

    try {
      // Create a channel for receiving events
      const channel = new Channel<GCEvent>();
      
      // Set up the channel message handler
      channel.onmessage = (event: GCEvent) => {
        this.handleBackendEvent(event);
      };
      
      // Subscribe to the backend event stream
      await invoke('subscribe_to_events', {
        eventChannel: channel
      });
      
      this.unsubscribe = () => {
        // Channel cleanup is handled automatically
      };
      
      this.isListening = true;
      console.log('Grand Central: Started listening to backend events');
    } catch (error) {
      console.error('Grand Central: Failed to start listening:', error);
      throw error;
    }
  }

  /**
   * Stop listening to events from the backend
   */
  async stopListening(): Promise<void> {
    if (!this.isListening) {
      return;
    }

    if (this.unsubscribe) {
      this.unsubscribe();
      this.unsubscribe = undefined;
    }

    this.isListening = false;
    console.log('Grand Central: Stopped listening to backend events');
  }

  /**
   * Handle events from the backend and emit them to frontend subscribers
   */
  private handleBackendEvent(event: GCEvent): void {
    try {
      switch (event.type) {
        case 'ptyOpened':
          this.emit('pty-opened', {
            pty_id: event.data.pid,
            runbook: event.data.runbook,
            block: event.data.block,
            created_at: Number(event.data.created_at)
          });
          break;

        case 'ptyClosed':
          this.emit('pty-closed', {
            pty_id: event.data.pty_id
          });
          break;

        // Only handle PTY events for now

        default:
          console.warn('Grand Central: Unhandled event type:', event);
      }
    } catch (error) {
      console.error('Grand Central: Error handling backend event:', error, event);
    }
  }

  /**
   * Get the listening status
   */
  get listening(): boolean {
    return this.isListening;
  }
}

// Global instance
export const grandCentral = new GrandCentral();

// Convenience functions for PTY events
export const onPtyOpened = (handler: (data: GrandCentralEvents['pty-opened']) => void) => 
  grandCentral.on('pty-opened', handler);

export const onPtyClosed = (handler: (data: GrandCentralEvents['pty-closed']) => void) => 
  grandCentral.on('pty-closed', handler);
