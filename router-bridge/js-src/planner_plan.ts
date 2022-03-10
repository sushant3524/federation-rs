import type { plan2 } from ".";
import type { OperationResult } from "./types";

/**
 * There are several global properties that we make available in our V8 runtime
 * and these are the types for those that we expect to use within this script.
 * They'll be stripped in the emitting of this file as JS, of course.
 */
declare let bridge: { plan2: typeof plan2 };

declare let done: (operationResult: OperationResult) => void;
declare let queryString: string;
declare let operationName: string | undefined;

try {
  const planResult = bridge.plan2(queryString, operationName);
  if (planResult.errors?.length > 0) {
    done({ Err: planResult.errors });
  } else {
    done({ Ok: planResult.data });
  }
} catch (e) {
  done({ Err: [e] });
}
