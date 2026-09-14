import { Eye, EyeOff } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { MdChevronRight, MdClose, MdWarningAmber } from "react-icons/md";
import type { ConnectionOption } from "@/components/network/shared";
import { SessionNetworkSection } from "@/components/sessions/SessionNetworkSection";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NumberInput } from "@/components/ui/number-input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { invoke } from "@/lib/invoke";
import type { ProxyConfig, SavedPassword } from "@/types/global";

export type VncScaleMode = "fit" | "actual" | "stretch";
export type VncSecurityMode = "auto" | "vnc-auth" | "none";

interface VncFormProps {
  host: string;
  setHost: (value: string) => void;
  port: number;
  setPort: (value: number) => void;
  passwordId: string;
  setPasswordId: (value: string) => void;
  password: string;
  setPassword: (value: string) => void;
  hasPassword: boolean;
  setHasPassword: (value: boolean) => void;
  scaleMode: VncScaleMode;
  setScaleMode: (value: VncScaleMode) => void;
  securityMode: VncSecurityMode;
  setSecurityMode: (value: VncSecurityMode) => void;
  shared: boolean;
  setShared: (value: boolean) => void;
  viewOnly: boolean;
  setViewOnly: (value: boolean) => void;
  clipboardEnabled: boolean;
  setClipboardEnabled: (value: boolean) => void;
  reconnectEnabled: boolean;
  setReconnectEnabled: (value: boolean) => void;
  reconnectMaxAttempts: number;
  setReconnectMaxAttempts: (value: number) => void;
  proxyId: string;
  setProxyId: (value: string) => void;
  proxies: ProxyConfig[];
  jumpHostId: string;
  setJumpHostId: (value: string) => void;
  jumpHostOptions: ConnectionOption[];
  connectionId?: string;
}

type PasswordSource = "direct" | "saved";
const MASKED_PASSWORD_PLACEHOLDER = "********";

function RequiredMark() {
  return <span className="ml-0.5 text-destructive">*</span>;
}

export function VncForm({
  host,
  setHost,
  port,
  setPort,
  passwordId,
  setPasswordId,
  password,
  setPassword,
  hasPassword,
  setHasPassword,
  scaleMode,
  setScaleMode,
  securityMode,
  setSecurityMode,
  shared,
  setShared,
  viewOnly,
  setViewOnly,
  clipboardEnabled,
  setClipboardEnabled,
  reconnectEnabled,
  setReconnectEnabled,
  reconnectMaxAttempts,
  setReconnectMaxAttempts,
  proxyId,
  setProxyId,
  proxies,
  jumpHostId,
  setJumpHostId,
  jumpHostOptions,
  connectionId,
}: VncFormProps) {
  const { t } = useTranslation();
  const [passwords, setPasswords] = useState<SavedPassword[]>([]);
  const [showPassword, setShowPassword] = useState(false);
  const [passwordLoading, setPasswordLoading] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [passwordSource, setPasswordSource] = useState<PasswordSource>(
    passwordId ? "saved" : "direct",
  );

  useEffect(() => {
    invoke<SavedPassword[]>("get_saved_passwords")
      .then((items) => {
        const usableItems = items.filter((item) => item.has_password === true);
        setPasswords(usableItems);
        if (passwordId && !usableItems.some((item) => item.id === passwordId)) {
          setPasswordId("");
        }
      })
      .catch(() => {});
  }, [passwordId, setPasswordId]);

  useEffect(() => {
    setPasswordSource(passwordId ? "saved" : "direct");
  }, [passwordId]);

  const togglePasswordVisibility = async () => {
    if (showPassword) {
      setShowPassword(false);
      return;
    }
    if (!password && hasPassword && connectionId) {
      setPasswordLoading(true);
      try {
        const value = await invoke<string | null>("get_connection_password_value", {
          id: connectionId,
        });
        if (value) {
          setPassword(value);
          setHasPassword(false);
        }
      } finally {
        setPasswordLoading(false);
      }
    }
    setShowPassword(true);
  };

  const selectedPasswordName = passwords.find((item) => item.id === passwordId)?.name;
  const insecureSecurity = securityMode === "none" || securityMode === "vnc-auth";

  return (
    <div className="w-full space-y-3">
      <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_8rem]">
        <div>
          <Label className="text-xs font-medium text-foreground/80">
            {t("dialog.host")}
            <RequiredMark />
          </Label>
          <Input
            className="mt-1 h-8 text-xs"
            placeholder="192.168.1.100"
            value={host}
            onChange={(event) => setHost(event.target.value)}
          />
        </div>
        <div>
          <Label className="text-xs font-medium text-foreground/80">
            {t("dialog.port")}
            <RequiredMark />
          </Label>
          <NumberInput
            className="mt-1 [&_button]:h-8 [&_button]:w-8 [&_input]:h-8 [&_input]:text-xs"
            value={port}
            onChange={setPort}
            min={1}
            max={65535}
          />
        </div>
      </div>

      <div>
        <Label className="text-xs font-medium text-foreground/80">
          {t("dialog.authentication")}
        </Label>
        <Tabs
          value={passwordSource}
          onValueChange={(value) => {
            const source = value as PasswordSource;
            setPasswordSource(source);
            if (source === "direct") {
              setPasswordId("");
            } else {
              setPassword("");
              setHasPassword(false);
              setShowPassword(false);
            }
          }}
          className="mt-1 w-full"
        >
          <TabsList className="grid h-8 w-full grid-cols-2 pointer-events-auto">
            <TabsTrigger value="direct" className="text-xs">
              {t("dialog.directPassword")}
            </TabsTrigger>
            <TabsTrigger value="saved" className="text-xs">
              {t("dialog.savedPassword")}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="direct" className="mt-3 border-0 outline-none">
            <Label className="text-xs font-medium text-foreground/80">{t("dialog.password")}</Label>
            <div className="relative mt-1">
              <Input
                className="h-8 pr-16 text-xs"
                data-custom-password-reveal
                type={showPassword ? "text" : "password"}
                value={password}
                placeholder={
                  hasPassword && !password
                    ? MASKED_PASSWORD_PLACEHOLDER
                    : t("dialog.passwordPlaceholder")
                }
                disabled={passwordLoading || securityMode === "none"}
                onChange={(event) => {
                  setPassword(event.target.value);
                  setPasswordId("");
                  if (event.target.value) setHasPassword(false);
                }}
              />
              {(password || hasPassword) && securityMode !== "none" ? (
                <>
                  <button
                    type="button"
                    className="absolute right-7 top-1/2 -translate-y-1/2 rounded-sm p-0.5 text-muted-foreground hover:text-foreground"
                    title={showPassword ? t("dialog.hidePassword") : t("dialog.showPassword")}
                    onClick={() => void togglePasswordVisibility()}
                  >
                    {showPassword ? (
                      <EyeOff className="h-3.5 w-3.5" />
                    ) : (
                      <Eye className="h-3.5 w-3.5" />
                    )}
                  </button>
                  <button
                    type="button"
                    className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded-sm p-0.5 text-muted-foreground hover:text-foreground"
                    title={t("dialog.clearPassword", "Clear password")}
                    onClick={() => {
                      setPassword("");
                      setHasPassword(false);
                      setShowPassword(false);
                    }}
                  >
                    <MdClose className="text-sm" />
                  </button>
                </>
              ) : null}
            </div>
            <p className="mt-1 text-[0.6875rem] text-muted-foreground">
              {t("dialog.vncPasswordLimit")}
            </p>
          </TabsContent>
          <TabsContent value="saved" className="mt-3 border-0 outline-none">
            <Select
              value={passwordId || "__none__"}
              disabled={securityMode === "none"}
              onValueChange={(value) => {
                setPasswordId(value === "__none__" ? "" : value);
                setPassword("");
                setHasPassword(false);
              }}
            >
              <SelectTrigger className="h-8 w-full text-xs font-normal">
                <SelectValue>{selectedPasswordName || t("dialog.none")}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">{t("dialog.none")}</SelectItem>
                {passwords.map((item) => (
                  <SelectItem key={item.id} value={item.id}>
                    {item.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </TabsContent>
        </Tabs>
      </div>

      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
        <CollapsibleTrigger className="group flex w-full items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground">
          <MdChevronRight
            className={`text-sm transition-transform ${advancedOpen ? "rotate-90" : ""}`}
          />
          <span>{t("dialog.advancedConfig")}</span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-3 space-y-3">
          <SessionNetworkSection
            proxyId={proxyId}
            setProxyId={setProxyId}
            proxies={proxies}
            jumpHostId={jumpHostId}
            setJumpHostId={setJumpHostId}
            jumpHostOptions={jumpHostOptions}
          />

          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <Label className="text-xs font-medium text-foreground/80">
                {t("dialog.vncScaling")}
              </Label>
              <Select
                value={scaleMode}
                onValueChange={(value) => setScaleMode(value as VncScaleMode)}
              >
                <SelectTrigger className="mt-1 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="fit">{t("dialog.vncScaleFit")}</SelectItem>
                  <SelectItem value="actual">{t("dialog.vncScaleActual")}</SelectItem>
                  <SelectItem value="stretch">{t("dialog.vncScaleStretch")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div>
              <Label className="text-xs font-medium text-foreground/80">
                {t("dialog.vncSecurity")}
              </Label>
              <Select
                value={securityMode}
                onValueChange={(value) => setSecurityMode(value as VncSecurityMode)}
              >
                <SelectTrigger className="mt-1 h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">{t("dialog.vncSecurityAuto")}</SelectItem>
                  <SelectItem value="vnc-auth">{t("dialog.vncSecurityAuth")}</SelectItem>
                  <SelectItem value="none">{t("dialog.vncSecurityNone")}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {insecureSecurity ? (
            <div className="flex gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-300">
              <MdWarningAmber className="mt-0.5 shrink-0 text-base" />
              <span>{t("dialog.vncUnencryptedWarning")}</span>
            </div>
          ) : null}

          <div className="grid gap-2 sm:grid-cols-2">
            {[
              [t("dialog.vncShared"), shared, setShared],
              [t("dialog.vncViewOnly"), viewOnly, setViewOnly],
              [t("dialog.vncClipboard"), clipboardEnabled, setClipboardEnabled],
              [t("dialog.vncAutoReconnect"), reconnectEnabled, setReconnectEnabled],
            ].map(([label, checked, setter]) => (
              <div
                key={String(label)}
                className="flex items-center justify-between rounded-md border px-3 py-2"
              >
                <span className="text-xs font-medium">{String(label)}</span>
                <Switch
                  checked={Boolean(checked)}
                  onCheckedChange={setter as (value: boolean) => void}
                />
              </div>
            ))}
          </div>
          <div className="max-w-xs">
            <Label className="text-xs font-medium text-foreground/80">
              {t("dialog.vncReconnectAttempts")}
            </Label>
            <NumberInput
              className="mt-1 [&_button]:h-8 [&_button]:w-8 [&_input]:h-8 [&_input]:text-xs"
              value={reconnectMaxAttempts}
              onChange={setReconnectMaxAttempts}
              min={0}
              max={20}
              disabled={!reconnectEnabled}
            />
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
