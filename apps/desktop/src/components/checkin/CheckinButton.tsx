import { Loader2, Play } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface CheckinButtonProps {
  /** 是否处于签到中（禁用重复点击） */
  loading: boolean;
  onClick: () => void;
  /** 按钮文案，默认「一键签到」 */
  label?: string;
  className?: string;
  size?: "default" | "sm" | "lg";
  disabled?: boolean;
}

export function CheckinButton({
  loading,
  onClick,
  label = "一键签到",
  className,
  size = "lg",
  disabled,
}: CheckinButtonProps) {
  return (
    <Button
      size={size}
      className={cn("min-w-[9rem]", className)}
      onClick={onClick}
      disabled={loading || disabled}
      aria-busy={loading}
    >
      {loading ? (
        <Loader2 className="h-4 w-4 animate-spin" />
      ) : (
        <Play className="h-4 w-4" />
      )}
      {loading ? "签到中…" : label}
    </Button>
  );
}
