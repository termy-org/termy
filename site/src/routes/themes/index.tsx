import { Link, createFileRoute } from "@tanstack/react-router";
import type { JSX } from "react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  type AuthUser,
  type Theme,
  type ThemePalette,
  fallbackPalette,
  fetchThemes,
  fetchCurrentUser,
  getThemeLoginUrl,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/")({
  component: ThemeStorePage,
});

function buildThemeInstallHref(slug: string): string {
  return `termy://store/theme-install?slug=${encodeURIComponent(slug)}`;
}

function ThemeColorSwatch({
  fileUrl,
}: {
  fileUrl: string | null;
}): JSX.Element {
  const [palette, setPalette] = useState<ThemePalette | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!fileUrl) {
      setLoading(false);
      return;
    }

    void fetch(fileUrl)
      .then((res) => res.json() as Promise<ThemePalette>)
      .then((json) => setPalette({ ...fallbackPalette, ...json }))
      .catch(() => setPalette(null))
      .finally(() => setLoading(false));
  }, [fileUrl]);

  const colors = palette ?? fallbackPalette;

  const swatchColors = [
    colors.black,
    colors.red,
    colors.green,
    colors.yellow,
    colors.blue,
    colors.magenta,
    colors.cyan,
    colors.white,
  ];

  if (loading) {
    return (
      <div
        className="flex items-end gap-1 p-4 sm:p-5 h-[100px]"
        style={{ background: fallbackPalette.background }}
      >
        {Array.from({ length: 8 }).map((_, i) => (
          <div
            key={i}
            className="flex-1 rounded-sm animate-pulse bg-white/10"
            style={{ height: `${30 + ((i * 17) % 50)}%` }}
          />
        ))}
      </div>
    );
  }

  return (
    <div
      className="flex items-end gap-1 p-4 sm:p-5 h-[100px]"
      style={{ background: colors.background ?? fallbackPalette.background }}
    >
      {swatchColors.map((color, i) => (
        <div
          key={i}
          className="flex-1 rounded-sm"
          style={{
            backgroundColor: color ?? fallbackPalette.black,
            height: `${30 + ((i * 17) % 50)}%`,
          }}
        />
      ))}
    </div>
  );
}

function ThemeStorePage(): JSX.Element {
  const [themes, setThemes] = useState<Theme[]>([]);
  const [user, setUser] = useState<AuthUser | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const loginUrl = useMemo(() => getThemeLoginUrl("/themes"), []);
  const filteredThemes = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();

    if (!query) {
      return themes;
    }

    return themes.filter((theme) =>
      [theme.name, theme.slug, theme.description, theme.githubUsernameClaim]
        .join(" ")
        .toLowerCase()
        .includes(query),
    );
  }, [searchQuery, themes]);

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
      <div className="mx-auto max-w-6xl space-y-10">
        {/* Hero */}
        <div className="text-center max-w-3xl mx-auto px-6">
          <h1
            className="text-4xl md:text-6xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            <span className="gradient-text">themes.</span>
          </h1>
          <p
            className="mt-4 text-lg text-muted-foreground animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            Browse community themes and preview each style in a terminal mockup.
          </p>
          <div
            className="mt-6 flex flex-wrap items-center justify-center gap-3 animate-blur-in"
            style={{ animationDelay: "200ms" }}
          >
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

        <div className="mx-auto w-full max-w-2xl px-6">
          <div className="rounded-2xl border border-border/60 bg-card/40 p-2 backdrop-blur-sm">
            <input
              type="search"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="Search themes by name, slug, description, or author..."
              className="w-full rounded-xl border border-transparent bg-background/70 px-4 py-3 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/70 focus:border-primary/40"
              aria-label="Search themes"
            />
          </div>
        </div>

        {/* Theme cards */}
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filteredThemes.map((theme, i) => (
            <div
              key={theme.id}
              className="animate-blur-in group flex flex-col rounded-xl border border-border/40 bg-card/30 transition-all duration-300 hover:border-primary/20 hover:bg-card/60 overflow-hidden"
              style={{ animationDelay: `${(i + 1) * 100}ms` }}
            >
              <Link
                to="/themes/$slug"
                params={{ slug: theme.slug }}
                className="flex flex-col"
              >
                <ThemeColorSwatch fileUrl={theme.fileUrl} />

                <div className="mx-4 sm:mx-5 border-t border-border/30" />

                <div className="p-4 sm:p-5 mt-auto flex flex-col gap-2">
                  <div className="flex items-center justify-between gap-2">
                    <h3 className="text-[15px] font-semibold text-foreground leading-tight truncate">
                      {theme.name}
                    </h3>
                    {theme.latestVersion && (
                      <span className="shrink-0 rounded bg-primary/10 px-2 py-0.5 text-xs text-primary">
                        {theme.latestVersion}
                      </span>
                    )}
                  </div>
                  <p className="line-clamp-2 text-sm text-muted-foreground/80 leading-relaxed">
                    {theme.description || "No description provided."}
                  </p>
                  <span className="text-[11px] text-primary/50 font-mono tracking-wide mt-1">
                    @{theme.githubUsernameClaim}
                  </span>
                </div>
              </Link>

              <div className="px-4 pb-4 sm:px-5 sm:pb-5">
                <Button asChild size="sm" className="w-full">
                  <a href={buildThemeInstallHref(theme.slug)}>Install</a>
                </Button>
              </div>
            </div>
          ))}
        </div>

        {!loading && themes.length === 0 && (
          <div className="rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
            No themes published yet.
          </div>
        )}

        {!loading && themes.length > 0 && filteredThemes.length === 0 && (
          <div className="rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
            No themes match "{searchQuery.trim()}".
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
