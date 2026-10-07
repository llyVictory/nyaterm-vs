import { runtime } from "@/lib/backend/runtime";
import { ChevronsUpDownIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { MdCheck } from "react-icons/md";
import { ConnectionCombobox, type ConnectionOption } from "@/components/network/shared";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import type { ProxyConfig } from "@/types/global";

function formatProxySubtitle(proxy: ProxyConfig) {
  if (proxy.protocol === "proxycommand") {
    return proxy.command || proxy.protocol;
  }
  const auth = proxy.username ? proxy.username : "";
  const target = `${proxy.host}:${proxy.port}`;
  return [proxy.protocol.toUpperCase(), target, auth].filter(Boolean).join(" · ");
}

function ProxyCombobox({
  value,
  proxies,
  onChange,
}: {
  value: string;
  proxies: ProxyConfig[];
  onChange: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const options = proxies.filter((proxy) => runtime === "desktop" || proxy.protocol !== "proxycommand").map((proxy) => ({
    id: proxy.id,
    label: proxy.name,
    searchText: [proxy.name, proxy.protocol, proxy.host, proxy.port, proxy.username, proxy.command]
      .filter(Boolean)
      .join(" "),
    subtitle: formatProxySubtitle(proxy),
  }));
  const selected = options.find((option) => option.id === value);
  const displayLabel = selected
    ? selected.label
    : value
      ? t("dialog.selectedItemMissing")
      : t("dialog.noProxy");

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className="h-auto min-h-10 w-full justify-between px-3 py-2 font-normal"
        >
          <div className="min-w-0 text-left">
            <div
              className={cn(
                "truncate text-sm",
                !selected && !value ? "text-muted-foreground" : "text-foreground",
              )}
            >
              {displayLabel}
            </div>
            {(selected || value) && (
              <div className="truncate text-xs text-muted-foreground">
                {selected?.subtitle ?? t("dialog.selectedItemMissing")}
              </div>
            )}
          </div>
          <ChevronsUpDownIcon className="ml-3 shrink-0 text-sm text-muted-foreground opacity-70" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        side="bottom"
        collisionPadding={16}
        className="w-[min(32rem,calc(100vw-2rem))] p-0"
      >
        <Command>
          <CommandInput placeholder={t("network.searchProxies")} />
          <CommandList className="max-h-72">
            <CommandEmpty>{t("network.noProxyConfigs")}</CommandEmpty>
            <CommandGroup className="p-0">
              <CommandItem
                value={t("dialog.noProxy")}
                className="items-start gap-3 px-3 py-2"
                onSelect={() => {
                  onChange("");
                  setOpen(false);
                }}
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm">{t("dialog.noProxy")}</div>
                </div>
                {!value ? <MdCheck className="mt-0.5 text-sm text-primary" /> : null}
              </CommandItem>
            </CommandGroup>
            <CommandGroup className="p-0">
              {options.map((option) => (
                <CommandItem
                  key={option.id}
                  value={`${option.label} ${option.searchText}`}
                  className="items-start gap-3 px-3 py-2"
                  onSelect={() => {
                    onChange(option.id);
                    setOpen(false);
                  }}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm">{option.label}</div>
                    <div className="truncate text-xs text-muted-foreground">{option.subtitle}</div>
                  </div>
                  {option.id === value ? <MdCheck className="mt-0.5 text-sm text-primary" /> : null}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

export function SessionNetworkSection({
  proxyId,
  setProxyId,
  proxies,
  jumpHostId,
  setJumpHostId,
  jumpHostOptions,
}: {
  proxyId: string;
  setProxyId: (value: string) => void;
  proxies: ProxyConfig[];
  jumpHostId: string;
  setJumpHostId: (value: string) => void;
  jumpHostOptions: ConnectionOption[];
}) {
  const { t } = useTranslation();

  return (
    <div className="space-y-3 rounded-lg border bg-accent/25 p-3">
      <div>
        <Label className="text-xs font-medium text-foreground/80">{t("dialog.proxySelect")}</Label>
        <div className="mt-1">
          <ProxyCombobox value={proxyId} proxies={proxies} onChange={setProxyId} />
        </div>
      </div>
      <div>
        <Label className="text-xs font-medium text-foreground/80">
          {t("dialog.selectProxyJump")}
        </Label>
        <div className="mt-1">
          <ConnectionCombobox
            value={jumpHostId}
            options={jumpHostOptions}
            placeholder={t("dialog.noProxyJump")}
            searchPlaceholder={t("network.searchConnections")}
            emptyText={t("dialog.proxyJumpSshOnly")}
            missingSelectionLabel={t("network.connectionMissing")}
            clearLabel={t("dialog.noProxyJump")}
            onChange={setJumpHostId}
          />
        </div>
      </div>
    </div>
  );
}
