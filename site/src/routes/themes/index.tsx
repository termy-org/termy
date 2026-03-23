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
        className="flex items-end gap-1 p-4 h-[100px]"
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
      className="flex items-end gap-1 p-4 h-[100px]"
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
    <section className="pt-24 pb-16">
      <div className="mx-auto max-w-6xl">
        {/* Header */}
        <div className="mb-8">
          <h1 className="text-3xl md:text-4xl font-bold">Themes</h1>
          <p className="mt-2 text-muted-foreground">
            Browse community themes for Termy.
          </p>
        </div>

        {/* Actions */}
        <div className="flex flex-wrap items-center gap-3 mb-6">
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
            <span className="text-sm text-muted-foreground">
              Signed in as <span className="font-medium">@{user.githubLogin}</span>
            </span>
          )}
        </div>

        {error && (
          <div className="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive mb-6">
            {error}
          </div>
        )}

        {/* Search */}
        <div className="max-w-xl mb-8">
          <input
            type="search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="Search themes..."
            className="w-full rounded-lg border border-border bg-background px-4 py-2 text-sm outline-none focus:border-primary/50"
          />
        </div>

        {/* Theme cards */}
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filteredThemes.map((theme) => (
            <div
              key={theme.id}
              className="group flex flex-col rounded-lg border border-border bg-card overflow-hidden"
            >
              <Link
                to="/themes/$slug"
                params={{ slug: theme.slug }}
                className="flex flex-col"
              >
                <ThemeColorSwatch fileUrl={theme.fileUrl} />

                <div className="p-4">
                  <div className="flex items-center justify-between gap-2 mb-1">
                    <h3 className="font-semibold truncate">{theme.name}</h3>
                    {theme.latestVersion && (
                      <span className="shrink-0 text-xs text-muted-foreground">
                        {theme.latestVersion}
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-muted-foreground line-clamp-2 mb-2">
                    {theme.description || "No description provided."}
                  </p>
                  <span className="text-xs text-muted-foreground font-mono">
                    @{theme.githubUsernameClaim}
                  </span>
                </div>
              </Link>

              <div className="px-4 pb-4">
                <Button asChild size="sm" className="w-full">
                  <a href={buildThemeInstallHref(theme.slug)}>Install</a>
                </Button>
              </div>
            </div>
          ))}
        </div>

        {!loading && themes.length === 0 && (
          <div className="text-center text-sm text-muted-foreground py-8">
            No themes published yet.
          </div>
        )}

        {!loading && themes.length > 0 && filteredThemes.length === 0 && (
          <div className="text-center text-sm text-muted-foreground py-8">
            No themes match "{searchQuery.trim()}".
          </div>
        )}

        {loading && (
          <div className="text-center text-sm text-muted-foreground py-8">
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
