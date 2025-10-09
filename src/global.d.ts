import type * as TsStdlib from "@binarymuse/ts-stdlib";

declare global {
  type Timeout = ReturnType<typeof setTimeout>;
  type Interval = ReturnType<typeof setInterval>;

  type Option<T> = TsStdlib.Option<T>;
  type Result<T, E> = TsStdlib.Result<T, E>;
  type Weak<T> = TsStdlib.Weak<T>;
  type RcInfo = TsStdlib.RcInfo;

  const Some: typeof TsStdlib.Some;
  const None: typeof TsStdlib.None;
  const Ok: typeof TsStdlib.Ok;
  const Err: typeof TsStdlib.Err;
  const Rc: typeof TsStdlib.Rc;
}

// This export is required to make this a module
export {};
