import { KeyRound, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { SavedAccount } from "@/types/global";

const MANUAL_ACCOUNT_VALUE = "__manual_account__";

interface AccountSelectorProps {
  accounts: SavedAccount[];
  value: string;
  onChange: (value: string) => void;
}

export function AccountSelector({ accounts, value, onChange }: AccountSelectorProps) {
  const { t } = useTranslation();
  const selected = accounts.find((account) => account.id === value);

  return (
    <div>
      <Label className="text-xs font-medium text-foreground/80">{t("dialog.account")}</Label>
      <Select
        value={value || MANUAL_ACCOUNT_VALUE}
        onValueChange={(next) => onChange(next === MANUAL_ACCOUNT_VALUE ? "" : next)}
      >
        <SelectTrigger className="mt-1 h-auto min-h-9 w-full py-1.5 text-xs font-normal">
          <SelectValue>
            {selected ? (
              <span className="flex min-w-0 items-center gap-2">
                <UserRound className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{selected.name}</span>
                <span className="truncate text-muted-foreground">
                  {selected.username || t("dialog.accountUsernameFallback")}
                </span>
              </span>
            ) : value ? (
              t("dialog.missingAccount")
            ) : (
              t("dialog.manualAccount")
            )}
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={MANUAL_ACCOUNT_VALUE}>{t("dialog.manualAccount")}</SelectItem>
          {value && !selected ? (
            <SelectItem value={value} disabled>
              {t("dialog.missingAccount")}
            </SelectItem>
          ) : null}
          {accounts.map((account) => (
            <SelectItem key={account.id} value={account.id}>
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate">{account.name}</span>
                <span className="truncate text-muted-foreground">
                  {account.username || t("dialog.accountUsernameFallback")}
                </span>
                <KeyRound className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span className="text-muted-foreground">
                  {account.has_password
                    ? t("dialog.accountHasPassword")
                    : t("dialog.accountNoPassword")}
                </span>
              </span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
