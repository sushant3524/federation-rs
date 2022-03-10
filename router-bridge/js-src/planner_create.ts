import type { create } from ".";
import type { OperationResult } from "./types";

/**
 * There are several global properties that we make available in our V8 runtime
 * and these are the types for those that we expect to use within this script.
 * They'll be stripped in the emitting of this file as JS, of course.
 */
declare let bridge: { create: typeof create };

declare let done: (operationResult: OperationResult) => void;
declare let schemaString: string;

try {
  bridge.create(schemaString);
  done({ Ok: null });
} catch (e) {
  done({ Err: [e] });
}
