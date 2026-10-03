import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { authenticate, startEvents } from "./http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function BrowserGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [ready, setReady] = useState(false);
  const [checking, setChecking] = useState(true);
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void authenticate()
      .then(startEvents)
      .then(() => {
        if (active) setReady(true);
      })
      .catch(() => {})
      .finally(() => {
        if (active) setChecking(false);
      });
    return () => {
      active = false;
    };
  }, []);
  if (ready) return children;
  return (
    <div className="flex h-screen items-center justify-center bg-background text-foreground">
      <form
        className="w-80 space-y-4 rounded-lg border p-6"
        onSubmit={(event) => {
          event.preventDefault();
          setChecking(true);
          setError("");
          void authenticate(password)
            .then(startEvents)
            .then(() => {
              setPassword("");
              setReady(true);
            })
            .catch(() => setError(t("web.signInFailed")))
            .finally(() => setChecking(false));
        }}
      >
        <h1 className="text-xl font-semibold">NyaTerm Web</h1>
        <label className="block space-y-2">
          <span>{t("web.password")}</span>
          <Input
            type="password"
            value={password}
            autoComplete="current-password"
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>
        {error && (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        )}
        <Button
          className="w-full"
          type="submit"
          disabled={checking || !password}
        >
          {t("web.signIn")}
        </Button>
      </form>
    </div>
  );
}
