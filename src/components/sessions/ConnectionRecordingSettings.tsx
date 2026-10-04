import { supports } from "@/lib/backend/runtime";
import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { RecordingMode } from "@/types/global";

interface ConnectionRecordingSettingsProps {
  useGlobal: boolean;
  onUseGlobalChange: (value: boolean) => void;
  autoStart: boolean;
  onAutoStartChange: (value: boolean) => void;
  mode: RecordingMode;
  onModeChange: (value: RecordingMode) => void;
}

export function ConnectionRecordingSettings({
  useGlobal,
  onUseGlobalChange,
  autoStart,
  onAutoStartChange,
  mode,
  onModeChange,
}: ConnectionRecordingSettingsProps) {
  const { t } = useTranslation();
  if (!supports("recording")) return null;

  return (
    <div className="border-t pt-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <Label className="text-xs font-medium text-foreground/80">
            {t("dialog.connectionRecording")}
          </Label>
          <p className="mt-0.5 text-[0.6875rem] leading-relaxed text-muted-foreground">
            {t("dialog.connectionRecordingDesc")}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="text-xs text-muted-foreground">{t("dialog.recordingUseGlobal")}</span>
          <Switch size="sm" checked={useGlobal} onCheckedChange={onUseGlobalChange} />
        </div>
      </div>

      {!useGlobal && (
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs font-medium">{t("dialog.recordingAutoStart")}</div>
              <p className="mt-0.5 text-[0.6875rem] leading-relaxed text-muted-foreground">
                {t("dialog.recordingAutoStartDesc")}
              </p>
            </div>
            <Switch
              className="mt-0.5"
              size="sm"
              checked={autoStart}
              onCheckedChange={onAutoStartChange}
            />
          </div>

          <div className="space-y-1.5">
            <Label className="text-xs font-medium text-foreground/80">
              {t("dialog.recordingMode")}
            </Label>
            <Select value={mode} onValueChange={(value) => onModeChange(value as RecordingMode)}>
              <SelectTrigger size="sm" className="h-8 w-full text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="transcript">{t("dialog.recordingModeTranscript")}</SelectItem>
                <SelectItem value="raw">{t("dialog.recordingModeRaw")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      )}
    </div>
  );
}
