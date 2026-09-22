import { beforeEach, expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  emit: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: mocks.emit,
  listen: mocks.listen,
}));

import {
  cancelCaptureRecovery,
  isCaptureRecoveryCancelled,
  listenCaptureRecoveryRequests,
  requestCaptureRecovery,
  subscribeCaptureRecoveryCancelled,
} from "./capture-recovery-requests";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.emit.mockResolvedValue(undefined);
  mocks.listen.mockResolvedValue(vi.fn());
});

test("broadcasts a recovery wakeup to the main window", async () => {
  await requestCaptureRecovery("session-1");

  expect(mocks.emit).toHaveBeenCalledWith("anlg:capture-recovery-request", {
    sessionId: "session-1",
  });
});

test("accepts only non-empty session identifiers", async () => {
  const onRequest = vi.fn();
  await listenCaptureRecoveryRequests(onRequest);
  const handler = mocks.listen.mock.calls[0]?.[1];

  handler?.({ payload: null });
  handler?.({ payload: { sessionId: "" } });
  handler?.({ payload: { sessionId: 42 } });
  handler?.({ payload: { sessionId: "session-1" } });

  expect(onRequest).toHaveBeenCalledOnce();
  expect(onRequest).toHaveBeenCalledWith("session-1");
});

test("a cancelled session neither broadcasts nor accepts recovery wakeups", async () => {
  const onRequest = vi.fn();
  await listenCaptureRecoveryRequests(onRequest);
  const handler = mocks.listen.mock.calls[0]?.[1];

  const release = cancelCaptureRecovery("session-1");
  expect(isCaptureRecoveryCancelled("session-1")).toBe(true);
  expect(isCaptureRecoveryCancelled("session-2")).toBe(false);

  await requestCaptureRecovery("session-1");
  handler?.({ payload: { sessionId: "session-1" } });
  expect(mocks.emit).not.toHaveBeenCalled();
  expect(onRequest).not.toHaveBeenCalled();

  await requestCaptureRecovery("session-2");
  handler?.({ payload: { sessionId: "session-2" } });
  expect(mocks.emit).toHaveBeenCalledOnce();
  expect(onRequest).toHaveBeenCalledWith("session-2");

  release();
  expect(isCaptureRecoveryCancelled("session-1")).toBe(false);
  await requestCaptureRecovery("session-1");
  handler?.({ payload: { sessionId: "session-1" } });
  expect(onRequest).toHaveBeenCalledWith("session-1");
});

test("overlapping cancellations only release once the last holder is done", () => {
  const first = cancelCaptureRecovery("session-1");
  const second = cancelCaptureRecovery("session-1");

  first();
  expect(isCaptureRecoveryCancelled("session-1")).toBe(true);

  second();
  expect(isCaptureRecoveryCancelled("session-1")).toBe(false);
});

test("cancelling notifies subscribers so pending recovery runs can be dropped", () => {
  const onCancelled = vi.fn();
  const unsubscribe = subscribeCaptureRecoveryCancelled(onCancelled);

  const release = cancelCaptureRecovery("session-1");
  expect(onCancelled).toHaveBeenCalledWith("session-1");

  release();
  unsubscribe();
  cancelCaptureRecovery("session-2")();
  expect(onCancelled).toHaveBeenCalledOnce();
});
