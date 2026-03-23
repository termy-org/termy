import { Link, createFileRoute } from "@tanstack/react-router";
import type { JSX } from "react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  type Theme,
  type ThemePalette,
  type ThemeVersion,
  fallbackPalette,
  fetchThemeWithVersions,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/$slug/")({
  component: ThemeDetailPage,
});

function buildThemeInstallHref(slug: string): string {
  return `termy://store/theme-install?slug=${encodeURIComponent(slug)}`;
}

function ThemeDetailPage(): JSX.Element {
  const { slug } = Route.useParams();

  const [theme, setTheme] = useState<Theme | null>(null);
  const [versions, setVersions] = useState<ThemeVersion[]>([]);
  const [palette, setPalette] = useState<ThemePalette>(fallbackPalette);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const latestFileUrl = useMemo(() => {
    if (theme?.fileUrl) {
      return theme.fileUrl;
    }

    return versions[0]?.fileUrl ?? null;
  }, [theme, versions]);

  useEffect(() => {
    void load();
  }, [slug]);

  useEffect(() => {
    if (!latestFileUrl) {
      setPalette(fallbackPalette);
      return;
    }

    void loadThemeJson(latestFileUrl);
  }, [latestFileUrl]);

  async function load(): Promise<void> {
    try {
      setLoading(true);
      setError(null);
      const response = await fetchThemeWithVersions(slug);
      setTheme(response.theme);
      setVersions(response.versions);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }

  async function loadThemeJson(url: string): Promise<void> {
    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Could not load theme file (${response.status})`);
      }
      const json = (await response.json()) as ThemePalette;
      setPalette({ ...fallbackPalette, ...json });
    } catch {
      setPalette(fallbackPalette);
    }
  }

  if (loading) {
    return (
      <section className="pt-28 pb-16">
        <div className="mx-auto max-w-6xl rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
          Loading theme...
        </div>
      </section>
    );
  }

  if (error || !theme) {
    return (
      <section className="pt-28 pb-16">
        <div className="mx-auto max-w-6xl space-y-4">
          <Button asChild variant="outline">
            <Link to="/themes">Back to store</Link>
          </Button>
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error ?? "Theme not found"}
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-6xl space-y-6">
        <div className="flex flex-wrap items-center gap-3">
          <Button asChild variant="outline">
            <Link to="/themes">Back to store</Link>
          </Button>
        </div>

        {/* Hero */}
        <div className="text-center max-w-3xl mx-auto px-6">
          <h1
            className="text-4xl md:text-6xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            <span className="gradient-text">{theme.name}</span>
          </h1>
          <p
            className="mt-4 text-lg text-muted-foreground animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            {theme.description || "No description provided."}
          </p>
          <div
            className="mt-4 flex flex-wrap items-center justify-center gap-3 text-sm text-muted-foreground animate-blur-in"
            style={{ animationDelay: "200ms" }}
          >
            <span>@{theme.githubUsernameClaim}</span>
            {theme.latestVersion && (
              <span className="rounded bg-primary/10 px-2 py-0.5 text-xs text-primary">
                Latest {theme.latestVersion}
              </span>
            )}
          </div>
          <div
            className="mt-6 flex flex-wrap items-center justify-center gap-3 animate-blur-in"
            style={{ animationDelay: "250ms" }}
          >
            <Button asChild>
              <a href={buildThemeInstallHref(theme.slug)}>Install in Termy</a>
            </Button>
            {latestFileUrl && (
              <Button asChild variant="outline">
                <a href={latestFileUrl} target="_blank" rel="noreferrer">
                  Download JSON
                </a>
              </Button>
            )}
          </div>
        </div>

        {/* Terminal preview */}
        <div className="animate-blur-in" style={{ animationDelay: "300ms" }}>
          <div
            className="terminal-window"
            style={{
              background: palette.background ?? fallbackPalette.background,
              borderColor: palette.bright_black ?? fallbackPalette.bright_black,
              boxShadow:
                "0 25px 50px -12px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.03)",
            }}
          >
            <div
              className="terminal-header"
              style={{
                background: palette.black ?? fallbackPalette.black,
                borderBottomColor:
                  palette.bright_black ?? fallbackPalette.bright_black,
              }}
            >
              <div className="terminal-dots">
                <div
                  className="terminal-dot"
                  style={{
                    backgroundColor: palette.red ?? fallbackPalette.red,
                  }}
                />
                <div
                  className="terminal-dot"
                  style={{
                    backgroundColor: palette.yellow ?? fallbackPalette.yellow,
                  }}
                />
                <div
                  className="terminal-dot"
                  style={{
                    backgroundColor: palette.green ?? fallbackPalette.green,
                  }}
                />
              </div>
              <span
                className="terminal-header-title"
                style={{
                  color: palette.foreground ?? fallbackPalette.foreground,
                }}
              >
                {theme.slug}
              </span>
            </div>
            <ThemePreviewTerminal palette={palette} theme={theme} />
          </div>
        </div>

        {/* Version History */}
        <div className="animate-blur-in" style={{ animationDelay: "400ms" }}>
          <div className="rounded-xl border border-border/40 bg-card/30 p-5 sm:p-6">
            <h2 className="text-lg font-semibold">Version History</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {versions.length} published versions
            </p>
            <div className="mt-4 space-y-2">
              {versions.map((version) => (
                <div
                  key={version.id}
                  className="rounded-lg border border-border/60 border-l-2 border-l-green-500/60 px-4 py-3"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{version.version}</span>
                      <span className="text-xs text-muted-foreground">
                        {new Date(version.publishedAt).toLocaleString()}
                      </span>
                    </div>
                    {version.fileUrl && (
                      <a
                        href={version.fileUrl}
                        target="_blank"
                        rel="noreferrer"
                        className="text-xs text-primary hover:underline"
                      >
                        Download JSON
                      </a>
                    )}
                  </div>
                  {version.changelog && (
                    <p className="mt-2 text-sm text-muted-foreground">
                      {version.changelog}
                    </p>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function ThemePreviewTerminal({
  palette,
  theme,
}: {
  palette: ThemePalette;
  theme: Theme;
}): JSX.Element {
  const colorRows = [
    [
      palette.black,
      palette.red,
      palette.green,
      palette.yellow,
      palette.blue,
      palette.magenta,
      palette.cyan,
      palette.white,
    ],
    [
      palette.bright_black,
      palette.bright_red,
      palette.bright_green,
      palette.bright_yellow,
      palette.bright_blue,
      palette.bright_magenta,
      palette.bright_cyan,
      palette.bright_white,
    ],
  ];

  return (
    <div
      className="terminal-body"
      style={{
        background: palette.background ?? fallbackPalette.background,
        color: palette.foreground ?? fallbackPalette.foreground,
      }}
    >
      <div className="space-y-1">
        <div>
          <span style={{ color: palette.green ?? fallbackPalette.green }}>
            $
          </span>{" "}
          theme preview {theme.slug}
        </div>
        <div>
          <span style={{ color: palette.blue ?? fallbackPalette.blue }}>→</span>{" "}
          latest version: {theme.latestVersion ?? "n/a"}
        </div>
        <div>
          <span style={{ color: palette.magenta ?? fallbackPalette.magenta }}>
            →
          </span>{" "}
          owner: @{theme.githubUsernameClaim}
        </div>
      </div>

      <div className="mt-4 font-mono text-[12px] leading-tight">
        <div style={{ color: palette.black ?? fallbackPalette.black }}>
          trace: using ANSI sample colors
        </div>
        <div style={{ color: palette.red ?? fallbackPalette.red }}>
          error: failed to connect
        </div>
        <div style={{ color: palette.yellow ?? fallbackPalette.yellow }}>
          warn: retrying with fallback profile
        </div>
        <div style={{ color: palette.green ?? fallbackPalette.green }}>
          ok: connection restored
        </div>
        <div style={{ color: palette.cyan ?? fallbackPalette.cyan }}>
          info: rendering with {theme.name}
        </div>
        <div style={{ color: palette.white ?? fallbackPalette.white }}>
          note: foreground/background loaded from theme JSON
        </div>
      </div>

      <div className="mt-4 space-y-1">
        {colorRows.map((row, rowIndex) => (
          <div key={rowIndex} className="flex">
            {row.map((color, colorIndex) => (
              <span
                key={colorIndex}
                className="inline-block h-5 flex-1"
                style={{
                  backgroundColor:
                    color ??
                    (rowIndex === 0
                      ? fallbackPalette.black
                      : fallbackPalette.bright_black),
                }}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Unexpected error";
}
