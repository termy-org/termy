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
  type Theme,
  type ThemeVersion,
  fetchThemeWithVersions,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/$slug")({
  component: ThemeDetailPage,
});

interface ThemePalette {
  foreground?: string;
  background?: string;
  cursor?: string;
  black?: string;
  red?: string;
  green?: string;
  yellow?: string;
  blue?: string;
  magenta?: string;
  cyan?: string;
  white?: string;
  bright_black?: string;
  bright_red?: string;
  bright_green?: string;
  bright_yellow?: string;
  bright_blue?: string;
  bright_magenta?: string;
  bright_cyan?: string;
  bright_white?: string;
}

const fallbackPalette: Required<ThemePalette> = {
  foreground: "#d1d5db",
  background: "#141a24",
  cursor: "#d1d5db",
  black: "#2e3436",
  red: "#cc0000",
  green: "#4e9a06",
  yellow: "#c4a000",
  blue: "#3465a4",
  magenta: "#75507b",
  cyan: "#06989a",
  white: "#d3d7cf",
  bright_black: "#555753",
  bright_red: "#ef2929",
  bright_green: "#8ae234",
  bright_yellow: "#fce94f",
  bright_blue: "#729fcf",
  bright_magenta: "#ad7fa8",
  bright_cyan: "#34e2e2",
  bright_white: "#eeeeec",
};

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

        <div className="rounded-3xl border border-border/50 bg-gradient-to-br from-card via-card to-secondary/50 p-6 md:p-8">
          <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
            Theme
          </p>
          <h1 className="mt-3 text-3xl font-semibold md:text-5xl">
            {theme.name}
          </h1>
          <p className="mt-3 max-w-3xl text-muted-foreground">
            {theme.description || "No description provided."}
          </p>
          <div className="mt-4 flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
            <span>@{theme.githubUsernameClaim}</span>
            {theme.latestVersion && (
              <span className="rounded bg-primary/10 px-2 py-0.5 text-xs text-primary">
                Latest {theme.latestVersion}
              </span>
            )}
          </div>
        </div>

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
                style={{ backgroundColor: palette.red ?? fallbackPalette.red }}
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

        <Card className="border-border/60">
          <CardHeader>
            <CardTitle>Version History</CardTitle>
            <CardDescription>
              {versions.length} published versions
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2">
            {versions.map((version) => (
              <div
                key={version.id}
                className="rounded-lg border border-border/60 px-3 py-3"
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
          </CardContent>
        </Card>
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
                className="inline-block h-5 w-8"
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
