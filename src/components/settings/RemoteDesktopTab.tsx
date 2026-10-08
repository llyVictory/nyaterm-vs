import { useTranslation } from "react-i18next";
import { SelectItem } from "@/components/ui/select";
import { useApp } from "@/context/AppContext";
import { canUseWindowsRdpClient } from "@/lib/backend/runtime";
import { SettingSection, SettingSelect } from "./SettingFormItems";

export function RemoteDesktopTab() {
  const { t } = useTranslation();
  const { appSettings, updateAppSettings } = useApp();
  if (!canUseWindowsRdpClient()) return null;

  return (
    <SettingSection>
      <SettingSelect
        label={t("settings.rdpDefaultClient")}
        desc={t("settings.rdpDefaultClientDesc")}
        value={appSettings.general.rdp_client_mode ?? "builtin"}
        onValueChange={(value) =>
          updateAppSettings({
            general: {
              ...appSettings.general,
              rdp_client_mode: value === "windows" ? "windows" : "builtin",
            },
          })
        }
      >
        <SelectItem value="builtin">
          {t("settings.rdpClientBuiltin")}
        </SelectItem>
        <SelectItem value="windows">
          {t("settings.rdpClientWindows")}
        </SelectItem>
      </SettingSelect>
      <p className="mt-3 text-xs text-muted-foreground">
        {t("settings.rdpSystemClientDesc")}
      </p>
    </SettingSection>
  );
}
