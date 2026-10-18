import { useCallback, useEffect, useRef, useState } from "react";
import {
  checkinRunAll,
  EVENTS,
  listen,
} from "@/lib/api";
import type { CheckinDoneEvent, CheckinProgressEvent } from "@/lib/types";

export type CheckinPhase = "idle" | "running" | "done" | "error";

export interface UseCheckinProgressState {
  phase: CheckinPhase;
  jobId: string | null;
  done: number;
  total: number;
  currentAccount?: string;
  status?: CheckinProgressEvent["status"];
  summary: CheckinDoneEvent | null;
  error: string | null;
}

export interface UseCheckinProgressReturn extends UseCheckinProgressState {
  /** 触发一键签到；运行中再次调用会被忽略 */
  runAll: (opts?: { platform?: "workbuddy" | "trae_cn" | "all" }) => Promise<void>;
  reset: () => void;
  isRunning: boolean;
}

const INITIAL: UseCheckinProgressState = {
  phase: "idle",
  jobId: null,
  done: 0,
  total: 0,
  currentAccount: undefined,
  status: undefined,
  summary: null,
  error: null,
};

export function useCheckinProgress(): UseCheckinProgressReturn {
  const [state, setState] = useState<UseCheckinProgressState>(INITIAL);
  const jobRef = useRef<string | null>(null);

  useEffect(() => {
    let alive = true;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const u1 = await listen<CheckinProgressEvent>(EVENTS.checkinProgress, (p) => {
        if (!alive) return;
        if (jobRef.current && p.jobId !== jobRef.current) return;
        setState((s) => ({
          ...s,
          phase: "running",
          jobId: p.jobId,
          done: p.done,
          total: p.total,
          currentAccount: p.currentAccount,
          status: p.status,
        }));
      });
      const u2 = await listen<CheckinDoneEvent>(EVENTS.checkinDone, (p) => {
        if (!alive) return;
        if (jobRef.current && p.jobId !== jobRef.current) return;
        setState((s) => ({
          ...s,
          phase: "done",
          jobId: p.jobId,
          summary: p,
          done: p.success + p.failed + (p.skipped ?? 0),
          total: Math.max(s.total, p.success + p.failed + (p.skipped ?? 0)),
        }));
        jobRef.current = null;
      });
      if (alive) {
        unlisteners.push(u1, u2);
      } else {
        u1();
        u2();
      }
    })();

    return () => {
      alive = false;
      for (const u of unlisteners) u();
    };
  }, []);

  const runAll = useCallback(
    async (opts?: { platform?: "workbuddy" | "trae_cn" | "all" }) => {
      if (jobRef.current || state.phase === "running") return;
      setState({
        ...INITIAL,
        phase: "running",
      });
      try {
        const { jobId } = await checkinRunAll(opts);
        jobRef.current = jobId;
        setState((s) => ({ ...s, jobId, phase: "running" }));
      } catch (e) {
        jobRef.current = null;
        setState({
          ...INITIAL,
          phase: "error",
          error: e instanceof Error ? e.message : String(e),
        });
      }
    },
    [state.phase],
  );

  const reset = useCallback(() => {
    jobRef.current = null;
    setState(INITIAL);
  }, []);

  return {
    ...state,
    runAll,
    reset,
    isRunning: state.phase === "running",
  };
}
