import { MessageRef, Socket, Channel, Push } from "phoenix";
import Logger from "@/lib/logger";
import Emittery from "emittery";
const logger = new Logger("Socket");
import { useStore } from "@/state/store";
import AtuinEnv from "./atuin_env";
import Backoff from "./lib/backoff";

export default class SocketManager extends Emittery {
  private static apiToken: string | null = null;
  private static instance: SocketManager;
  private handlers: MessageRef[] = [];
  private store: typeof useStore;
  private socket: Socket;

  static get() {
    if (!SocketManager.instance) {
      SocketManager.instance = new SocketManager(SocketManager.apiToken);
    }

    return SocketManager.instance;
  }

  static setApiToken(token: string | null) {
    SocketManager.apiToken = token;

    if (SocketManager.instance) {
      SocketManager.instance.putNewApiToken(token);
    }
  }

  constructor(apiToken: string | null) {
    super();
    this.store = useStore;
    this.socket = this.buildSocket(apiToken);
    this.setupHandlers();
    if (apiToken) {
      this.connect();
    } else {
      this.store.getState().setConnectedToHubSocket(false);
    }
  }

  public channel(topic: string, channelParams?: object) {
    return new WrappedChannel(this, topic, channelParams);
  }

  public onSocketChange(callback: (socket: Socket) => void) {
    return this.on("socketchange", callback);
  }

  public onConnect(callback: (socket: Socket) => void) {
    if (this.isConnected()) {
      // Defer the callback to allow other handlers to be set up
      // before handling the connection event.
      setTimeout(() => {
        callback(this.socket);
      }, 0);
    }

    return this.on("connect", callback);
  }

  public onDisconnect(callback: (socket: Socket) => void) {
    return this.on("disconnect", callback);
  }

  public getSocket() {
    return this.socket;
  }

  public isConnected() {
    return this.socket.isConnected();
  }

  private putNewApiToken(token: string | null) {
    logger.info("Preparing new Socket with updated token");
    const newSocket = this.buildSocket(token);
    const previouslyConnected = this.socket.isConnected();
    this.socket.disconnect();
    this.cleanupHandlers();

    this.socket = newSocket;
    this.setupHandlers();

    if (token) {
      this.connect();
    } else if (previouslyConnected) {
      this.store.getState().setConnectedToHubSocket(false);
      this.emit("disconnect", this.socket);
    }
    this.emit("socketchange", this.socket);
  }

  private connect() {
    logger.debug(
      `Connecting to Atuin Hub with token: ${SocketManager.apiToken!.substring(0, 12)}...`,
    );
    this.socket.connect();
  }

  private buildSocket(apiToken: string | null) {
    const url = `${AtuinEnv.websocketProtocol}://${AtuinEnv.hubDomain}/sockets/doc`;

    return new Socket(url, {
      params: { token: apiToken },
    });
  }

  private setupHandlers() {
    this.handlers.push(
      this.socket.onOpen(() => {
        this.store.getState().setConnectedToHubSocket(true);
        this.emit("connect", this.socket);
      }),
    );
    this.handlers.push(
      this.socket.onClose(() => {
        this.store.getState().setConnectedToHubSocket(false);
        this.emit("disconnect", this.socket);
      }),
    );
  }

  private cleanupHandlers() {
    this.socket.off(this.handlers);
    this.handlers = [];
  }
}

export class WrappedChannel<J = unknown> {
  private topic: string;
  private manager: SocketManager;
  private emitter: Emittery = new Emittery();
  private internalEmitter: Emittery = new Emittery();
  private handlers: Map<string, { count: number; ref: number }> = new Map();
  private channelParams: object | undefined;
  private channel: Channel;
  private joinFailed: boolean = false;
  private joinBackoff: Backoff = new Backoff();
  private joinPromise: Promise<J> | null = null;

  constructor(manager: SocketManager, topic: string, channelParams?: object) {
    this.manager = manager;
    this.topic = topic;
    this.emitter = new Emittery();
    this.channelParams = channelParams;
    this.channel = this.manager.getSocket().channel(topic, channelParams);

    this.manager.onSocketChange(this.handleNewSocket.bind(this));
  }

  public onJoin(callback: () => void) {
    return this.internalEmitter.on("join", callback);
  }

  public nextJoin() {
    return this.internalEmitter.once("join");
  }

  // Internal method to join the channel;
  // consumers should use `ensureJoined` instead
  private async join(timeout: number = 10000): Promise<J> {
    if (this.joinFailed) {
      this.resetChannel();
      await this.joinBackoff.next();
    } else if (this.joinPromise) {
      return this.joinPromise;
    }

    const promise = new Promise<J>((resolve, reject) => {
      this.channel
        .join(timeout)
        .receive("ok", (resp: J) => {
          this.joinBackoff.reset();
          this.internalEmitter.emitSerial("join");
          resolve(resp);
        })
        .receive("error", (resp: any) => {
          this.joinFailed = true;
          this.channel.leave();
          if (resp.can_retry) {
            logger.debug("Retrying join...");
            this.join(timeout);
          }
          reject(resp);
        });
    });

    this.joinPromise = promise;

    // This is a hack to prevent unhandled rejection errors
    promise.catch(() => {});

    return promise;
  }

  public async ensureJoined(timeout: number = 10000): Promise<J> {
    const channelState = this.channel!.state;

    switch (channelState) {
      case "joined":
        return this.joinPromise!;
      case "joining":
        return this.joinPromise!;
      case "leaving":
        return this.join(timeout);
      case "closed":
        return this.join(timeout);
      case "errored":
        return this.join(timeout);
      default:
        const exhaustiveCheck: never = channelState;
        throw new Error(`Unhandled channel state: ${exhaustiveCheck}`);
    }
  }

  private resetChannel() {
    this.handleNewSocket(this.manager.getSocket());
  }

  public leave(timeout?: number) {
    const push = this.channel.leave(timeout);
    return new WrappedPush(Promise.resolve(push));
  }

  public on(event: string, callback: (response?: any) => void) {
    const unsub = this.emitter.on(event, callback);

    if (!this.handlers.has(event)) {
      const ref = this.channel.on(event, (response?: any) => {
        if (response && response instanceof ArrayBuffer) {
          this.emitter.emit(event, new Uint8Array(response));
        } else {
          this.emitter.emit(event, response);
        }
      });
      this.handlers.set(event, { count: 1, ref });
    } else {
      this.handlers.get(event)!.count++;
    }

    return () => {
      unsub();
      if (!this.handlers.has(event)) {
        console.warn(`${event} wasn't found in handlers, but an unsub was called.`);
      } else if (this.handlers.get(event)!.count === 1) {
        this.channel.off(event, this.handlers.get(event)!.ref);
        this.handlers.delete(event);
      } else {
        this.handlers.get(event)!.count--;
      }
    };
  }

  public push<T = any>(event: string, payload: T, timeout?: number): WrappedPush {
    let data: any = payload;
    if (data instanceof Uint8Array) {
      data = data.buffer;
    }

    const pushPromise = this.ensureJoined().then(() => {
      return this.channel.push(event, data, timeout);
    });

    return new WrappedPush(pushPromise);
  }

  public get state() {
    return this.channel.state;
  }

  private handleNewSocket(socket: Socket) {
    logger.debug(`Resetting channel ${this.topic}`);
    const oldChannel = this.channel;
    this.channel = socket.channel(this.topic, this.channelParams);
    this.joinFailed = false;
    this.joinPromise = null;

    this.handlers.forEach(({ ref }, event) => {
      oldChannel.off(event, ref);
      const newRef = this.channel.on(event, (response?: any) => this.emitter.emit(event, response));
      this.handlers.get(event)!.ref = newRef;
    });
  }
}

export class WrappedPush {
  private pushPromise: Promise<Push>;

  constructor(pushPromise: Promise<Push>) {
    this.pushPromise = pushPromise;
    // Prevent unhandled rejection for fire-and-forget pushes
    this.pushPromise.catch(() => {});
  }

  async receive<T = any>(): Promise<T> {
    const push = await this.pushPromise;
    return new Promise<T>((resolve, reject) => {
      push
        .receive("ok", (data: T) => resolve(data))
        .receive("error", (error: any) => reject(error));
    });
  }

  async receiveBin(): Promise<Uint8Array> {
    const push = await this.pushPromise;
    return new Promise((resolve, reject) => {
      push
        .receive("ok", (data: ArrayBuffer) => resolve(new Uint8Array(data)))
        .receive("error", (error: any) => reject(error));
    });
  }
}
