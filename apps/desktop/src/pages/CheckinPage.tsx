import { PageHeader } from "@/components/layout/PageHeader";
import { CheckinButton } from "@/components/checkin/CheckinButton";
import { CheckinProgress } from "@/components/checkin/CheckinProgress";
import { useCheckinProgress } from "@/hooks/useCheckinProgress";
import { Button } from "@/components/ui/button";
import { useEffect, useState } from "react";
import {
  ALLOW_LIVE_HTTP_CHANGED_EVENT,
  getAllowLiveHttp,
} from "@/lib/checkinSettings";

export function CheckinPage() {
  const checkin = useCheckinProgress();
  const [allowLiveHttp, setAllowLiveHttp] = useState(() => getAllowLiveHttp());

  useEffect(() => {
    const onChange = () => setAllowLiveHttp(getAllowLiveHttp());
    window.addEventListener(ALLOW_LIVE_HTTP_CHANGED_EVENT, onChange);
    return () => window.removeEventListener(ALLOW_LIVE_HTTP_CHANGED_EVENT, onChange);
  }, []);

  return (
    <div>
      <PageHeader
        title="签到"
        description={
          allowLiveHttp
            ? "批量一键签到与本次运行结果（allow_live_http 已开 · live）。"
            : "批量一键签到与本次运行结果（allow_live_http 关闭 · 仅 dry-run）。"
        }
        actions={
          <>
            {checkin.phase === "done" || checkin.phase === "error" ? (
              <Button variant="ghost" size="sm" onClick={checkin.reset}>
                重置
              </Button>
            ) : null}
            <CheckinButton loading={checkin.isRunning} onClick={() => void checkin.runAll()} />
          </>
        }
      />

      <CheckinProgress
        phase={checkin.phase}
        done={checkin.done}
        total={checkin.total}
        currentAccount={checkin.currentAccount}
        status={checkin.status}
        summary={checkin.summary}
        error={checkin.error}
      />

      <div className="mt-6 rounded-lg border border-dashed border-border p-6 text-sm text-fg-subtle">
        历史签到 run 列表（占位）— 待 storage / checkin_run 接口
      </div>
    </div>
  );
}
