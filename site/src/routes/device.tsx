import { createFileRoute } from "@tanstack/react-router";
import { ExternalLink, LoaderCircle, LogIn } from "lucide-react";
import type { JSX } from "react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  buildNativeAuthCallbackUrl,
  createDeviceSession,
  fetchCurrentUser,
  getThemeLoginUrl,
  type AuthUser,
} from "@/lib/theme-store";

export const Route = createFileRoute("/device")({
  component: DeviceAuthPage,
});

type DeviceState = "checking" | "signed_out" | "creating" | "ready" | "error";

function DeviceAuthPage(): JSX.Element {
  const [state, setState] = useState<DeviceState>("checking");
  const [user, setUser] = useState<AuthUser | null>(null);
  const [callbackUrl, setCallbackUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loginUrl = useMemo(() => getThemeLoginUrl("/device"), []);

  useEffect(() => {
    let cancelled = false;

    async function run(): Promise<void> {
      try {
        setState("checking");
        setError(null);
        const currentUser = await fetchCurrentUser();
        if (cancelled) {
          return;
        }

        if (!currentUser) {
          setUser(null);
          setState("signed_out");
          return;
        }

        setUser(currentUser);
        setState("creating");
        const deviceSession = await createDeviceSession();
        if (cancelled) {
          return;
        }

        const nextCallbackUrl = buildNativeAuthCallbackUrl(deviceSession);
        setCallbackUrl(nextCallbackUrl);
        setState("ready");
        window.location.replace(nextCallbackUrl);
      } catch (err) {
        if (cancelled) {
          return;
        }
        setError(getErrorMessage(err));
        setState("error");
      }
    }

    void run();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section className="relative overflow-hidden py-24 sm:py-32">
      <div className="absolute inset-0 -z-10 bg-[radial-gradient(circle_at_top,#93c5fd22,transparent_45%),linear-gradient(180deg,transparent,rgba(12,18,28,0.08))]" />

      <div className="mx-auto flex max-w-3xl flex-col items-center px-6 text-center">
        <div className="animate-blur-in rounded-full border border-border/50 bg-background/80 px-3 py-1 text-xs uppercase tracking-[0.28em] text-muted-foreground">
          Termy Device Login
        </div>

        <h1
          className="mt-6 text-4xl font-bold tracking-tight text-foreground md:text-6xl animate-blur-in"
          style={{ animationDelay: "80ms" }}
        >
          Connect your browser to <span className="gradient-text">Termy</span>.
        </h1>

        <p
          className="mt-4 max-w-2xl text-base leading-7 text-muted-foreground md:text-lg animate-blur-in"
          style={{ animationDelay: "140ms" }}
        >
          Sign in with GitHub here, then this page will hand the session back to the app with a
          secure `termy://auth/callback` deeplink.
        </p>

        <div
          className="mt-10 w-full animate-blur-in rounded-2xl border border-border/50 bg-card/40 p-6 shadow-[0_20px_80px_rgba(0,0,0,0.08)] backdrop-blur"
          style={{ animationDelay: "220ms" }}
        >
          {state === "checking" && <StatusPanel title="Checking browser session" />}
          {state === "creating" && (
            <StatusPanel
              title={user ? `Creating device session for @${user.githubLogin}` : "Creating device session"}
            />
          )}

          {state === "signed_out" && (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                You are not signed in yet. Continue with GitHub in the browser, then this page
                will return to Termy automatically.
              </p>
              <Button asChild size="lg" className="w-full sm:w-auto">
                <a href={loginUrl}>
                  <LogIn className="mr-2 size-4" />
                  Login with GitHub
                </a>
              </Button>
            </div>
          )}

          {state === "ready" && callbackUrl && (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                If Termy did not open automatically, open it manually with the button below.
              </p>
              <Button asChild size="lg" className="w-full sm:w-auto">
                <a href={callbackUrl}>
                  <ExternalLink className="mr-2 size-4" />
                  Open Termy
                </a>
              </Button>
              <div className="rounded-xl border border-border/50 bg-background/60 p-3 text-left">
                <p className="mb-2 text-xs uppercase tracking-[0.2em] text-muted-foreground">
                  Callback
                </p>
                <code className="break-all text-xs text-foreground/80">{callbackUrl}</code>
              </div>
            </div>
          )}

          {state === "error" && (
            <div className="space-y-4">
              <p className="text-sm text-destructive">{error ?? "Could not create device session."}</p>
              <div className="flex flex-wrap justify-center gap-3">
                <Button asChild>
                  <a href={loginUrl}>Retry GitHub Login</a>
                </Button>
                <Button variant="outline" onClick={() => window.location.reload()}>
                  Reload
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function StatusPanel({ title }: { title: string }): JSX.Element {
  return (
    <div className="flex flex-col items-center gap-4 py-6">
      <div className="rounded-full border border-border/50 bg-background/80 p-3">
        <LoaderCircle className="size-5 animate-spin text-primary" />
      </div>
      <p className="text-sm text-muted-foreground">{title}</p>
    </div>
  );
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Unexpected error";
}
