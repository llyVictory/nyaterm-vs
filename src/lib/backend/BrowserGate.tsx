import { Eye, EyeOff, LoaderCircle, LockKeyhole } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { logger } from "@/lib/logger";
import { authenticate, BackendRequestError, startEvents } from "./http";

export function BrowserGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [ready, setReady] = useState(false);
  const [phase, setPhase] = useState<"checking" | "idle" | "signingIn" | "connecting">("checking");
  const [authenticated, setAuthenticated] = useState(false);
  const [password, setPassword] = useState("");
  const [visible, setVisible] = useState(false);
  const [errorKey, setErrorKey] = useState("");
  const inFlight = useRef(false);
  const alive = useRef(true);
  const inputRef = useRef<HTMLInputElement>(null);
  const busy = phase !== "idle";

  useEffect(() => {
    let active = true;
    alive.current = true;
    void (async () => {
      try {
        await authenticate();
        if (!active) return;
        setAuthenticated(true);
        setPhase("connecting");
        try {
          await startEvents();
          if (active) setReady(true);
        } catch (error) {
          logger.warn({
            domain: "app.lifecycle",
            event: "web.events_unavailable",
            message: "Web event connection failed",
            error,
          });
          if (active) setErrorKey("web.eventConnectionFailed");
        }
      } catch (error) {
        if (active && (!(error instanceof BackendRequestError) || error.status !== 401)) {
          setErrorKey(
            error instanceof BackendRequestError && error.status === 429
              ? "web.signInRateLimited"
              : "web.serverUnavailable",
          );
        }
      } finally {
        if (active) setPhase("idle");
      }
    })();
    return () => {
      active = false;
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    if (phase === "idle" && !authenticated) inputRef.current?.focus();
  }, [phase, authenticated]);

  const submit = async () => {
    if (busy || inFlight.current || (!authenticated && !password)) return;
    inFlight.current = true;
    setErrorKey("");
    try {
      if (!authenticated) {
        setPhase("signingIn");
        try {
          await authenticate(password);
          if (!alive.current) return;
          setPassword("");
          setAuthenticated(true);
        } catch (error) {
          logger.warn({
            domain: "security.flow",
            event: "web.sign_in_failed",
            message: "Web sign-in failed",
            error,
          });
          if (alive.current)
            setErrorKey(
              error instanceof BackendRequestError && error.status === 401
                ? "web.invalidPassword"
                : error instanceof BackendRequestError && error.status === 429
                  ? "web.signInRateLimited"
                  : "web.serverUnavailable",
            );
          return;
        }
      }
      setPhase("connecting");
      try {
        await startEvents();
        if (alive.current) setReady(true);
      } catch (error) {
        logger.warn({
          domain: "app.lifecycle",
          event: "web.events_unavailable",
          message: "Web event connection failed",
          error,
        });
        if (alive.current) setErrorKey("web.eventConnectionFailed");
      }
    } finally {
      inFlight.current = false;
      if (alive.current) setPhase("idle");
    }
  };

  if (ready) return children;
  return (
    <main className="flex min-h-dvh items-center justify-center bg-background px-5 py-10 text-foreground">
      <div className="w-full max-w-[380px]">
        <div className="mb-7 flex items-center gap-3">
          <img
            src={`${import.meta.env.BASE_URL}icons/app/nyaterm.svg`}
            alt=""
            className="size-10"
          />
          <div>
            <p className="text-lg font-semibold tracking-tight">NyaTerm</p>
            <p className="text-xs text-muted-foreground">Web</p>
          </div>
        </div>
        <section className="rounded-xl border bg-card p-6 shadow-sm sm:p-7" aria-busy={busy}>
          <div className="mb-6">
            <h1 className="text-xl font-semibold tracking-tight">{t("web.welcome")}</h1>
            <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
              {t("web.signInDescription")}
            </p>
          </div>
          <form
            className="space-y-5"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            {!authenticated && (
              <div className="space-y-2">
                <Label htmlFor="web-password">{t("web.password")}</Label>
                <div className="relative">
                  <Input
                    ref={inputRef}
                    id="web-password"
                    type={visible ? "text" : "password"}
                    className="h-11 pr-11"
                    value={password}
                    autoComplete="current-password"
                    disabled={busy}
                    aria-invalid={errorKey === "web.invalidPassword"}
                    aria-describedby={errorKey ? "web-login-error" : undefined}
                    onChange={(event) => setPassword(event.target.value)}
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="absolute right-1 top-1 size-9 text-muted-foreground"
                    disabled={busy}
                    aria-label={t(visible ? "web.hidePassword" : "web.showPassword")}
                    aria-pressed={visible}
                    onClick={() => setVisible(!visible)}
                  >
                    {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                  </Button>
                </div>
              </div>
            )}
            {errorKey && (
              <p
                id="web-login-error"
                role="alert"
                className="text-sm leading-relaxed text-destructive"
              >
                {t(errorKey)}
              </p>
            )}
            <Button
              className="h-11 w-full"
              type="submit"
              disabled={busy || (!authenticated && !password)}
            >
              {busy && <LoaderCircle className="size-4 animate-spin" />}
              {t(
                phase === "checking"
                  ? "web.checkingSession"
                  : phase === "signingIn"
                    ? "web.signingIn"
                    : phase === "connecting"
                      ? "web.connecting"
                      : authenticated
                        ? "web.retryConnection"
                        : "web.signIn",
              )}
            </Button>
            {busy && (
              <output className="sr-only">
                {t(
                  phase === "checking"
                    ? "web.checkingSession"
                    : phase === "connecting"
                      ? "web.connecting"
                      : "web.signingIn",
                )}
              </output>
            )}
          </form>
        </section>
        <p className="mt-5 flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
          <LockKeyhole className="size-3.5" />
          {t("web.privateWorkspace")}
        </p>
      </div>
    </main>
  );
}
