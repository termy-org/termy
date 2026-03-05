import { Link, createFileRoute } from "@tanstack/react-router";
import type { JSX } from "react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  type AuthUser,
  type Theme,
  fetchCurrentUser,
  fetchThemes,
  getThemeLoginUrl,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/")({
  component: ThemeStorePage,
});

function ThemeStorePage(): JSX.Element {
  const [themes, setThemes] = useState<Theme[]>([]);
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loginUrl = useMemo(() => getThemeLoginUrl("/themes"), []);

  useEffect(() => {
    void load();
  }, []);

  async function load(): Promise<void> {
    try {
      setLoading(true);
      setError(null);
      const [themesResult, userResult] = await Promise.all([
        fetchThemes(),
        fetchCurrentUser().catch(() => null),
      ]);
      setThemes(themesResult);
      setUser(userResult);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-6xl space-y-6">
        <div className="rounded-3xl border border-border/50 bg-gradient-to-br from-card via-card to-secondary/50 p-6 md:p-8">
          <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
            Theme Store
          </p>
          <h1 className="mt-3 text-3xl font-semibold md:text-5xl">
            Discover community themes
          </h1>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            Browse published themes and preview each style in a terminal mockup.
          </p>
          <div className="mt-5 flex flex-wrap items-center gap-3">
            <Button asChild>
              <Link to="/add">Add your theme</Link>
            </Button>
            <Button asChild variant="outline">
              <Link to="/themes/studio">Theme Studio</Link>
            </Button>
            {!user && (
              <a href={loginUrl}>
                <Button variant="outline">Login with GitHub</Button>
              </a>
            )}
            {user && (
              <div className="rounded-lg border border-border/60 bg-background/80 px-3 py-2 text-sm">
                Signed in as{" "}
                <span className="font-medium">@{user.githubLogin}</span>
              </div>
            )}
          </div>
        </div>

        {error && (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {themes.map((theme) => (
            <Card key={theme.id} className="border-border/60">
              <CardHeader>
                <CardTitle className="flex items-center justify-between gap-2 text-base">
                  <span className="truncate">{theme.name}</span>
                  {theme.latestVersion && (
                    <span className="rounded bg-primary/10 px-2 py-0.5 text-xs text-primary">
                      {theme.latestVersion}
                    </span>
                  )}
                </CardTitle>
                <CardDescription>@{theme.githubUsernameClaim}</CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <p className="line-clamp-3 text-sm text-muted-foreground">
                  {theme.description || "No description provided."}
                </p>
                <Button asChild variant="outline" className="w-full">
                  <Link to="/themes/$slug" params={{ slug: theme.slug }}>
                    View theme
                  </Link>
                </Button>
              </CardContent>
            </Card>
          ))}
        </div>

        {!loading && themes.length === 0 && (
          <div className="rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
            No themes published yet.
          </div>
        )}

        {loading && (
          <div className="rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
            Loading themes...
          </div>
        )}
      </div>
    </section>
  );
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Unexpected error";
}
