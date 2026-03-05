import { Link, createFileRoute } from "@tanstack/react-router";
import type { JSX } from "react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export const Route = createFileRoute("/themes/studio")({
  component: ThemeStudioPage,
});

type ThemePalette = {
  foreground: string;
  background: string;
  cursor: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  bright_black: string;
  bright_red: string;
  bright_green: string;
  bright_yellow: string;
  bright_blue: string;
  bright_magenta: string;
  bright_cyan: string;
  bright_white: string;
};

const fallbackPalette: ThemePalette = {
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

const paletteFields = [
  "foreground",
  "background",
  "cursor",
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "bright_black",
  "bright_red",
  "bright_green",
  "bright_yellow",
  "bright_blue",
  "bright_magenta",
  "bright_cyan",
  "bright_white",
] as const satisfies readonly (keyof ThemePalette)[];

function ThemeStudioPage(): JSX.Element {
  const [palette, setPalette] = useState<ThemePalette>(fallbackPalette);

  const schemaJson = useMemo(() => {
    return JSON.stringify(
      {
        $schema: "http://json-schema.org/draft-07/schema#",
        ...palette,
      },
      null,
      2,
    );
  }, [palette]);

  function handleColorChange(key: keyof ThemePalette, value: string): void {
    setPalette((previous) => ({
      ...previous,
      [key]: normalizeHex(value, previous[key]),
    }));
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-6xl space-y-6">
        <div className="rounded-3xl border border-border/50 bg-gradient-to-br from-card via-card to-secondary/50 p-6 md:p-8">
          <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
            Theme Studio
          </p>
          <h1 className="mt-3 text-3xl font-semibold md:text-5xl">
            Build a theme live
          </h1>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            Tune every color and see the terminal preview update instantly.
          </p>
          <div className="mt-5 flex flex-wrap items-center gap-3">
            <Button asChild variant="outline">
              <Link to="/themes">Back to store</Link>
            </Button>
          </div>
        </div>

        <div className="grid gap-6 lg:grid-cols-[380px_minmax(0,1fr)]">
          <Card className="border-border/60">
            <CardHeader>
              <CardTitle>Palette</CardTitle>
            </CardHeader>
            <CardContent className="max-h-[700px] space-y-3 overflow-auto pr-1">
              {paletteFields.map((field) => (
                <label key={field} className="grid grid-cols-[1fr_auto] gap-3">
                  <div>
                    <p className="text-sm font-medium">{field}</p>
                    <input
                      className="mt-1 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm font-mono"
                      value={palette[field]}
                      onChange={(event) =>
                        handleColorChange(field, event.target.value)
                      }
                    />
                  </div>
                  <input
                    type="color"
                    value={safeColorValue(palette[field])}
                    onChange={(event) => handleColorChange(field, event.target.value)}
                    className="mt-6 h-10 w-10 cursor-pointer rounded border border-border bg-background p-1"
                    aria-label={`Select ${field} color`}
                  />
                </label>
              ))}
            </CardContent>
          </Card>

          <div className="space-y-6">
            <div
              className="terminal-window"
              style={{
                background: palette.background,
                borderColor: palette.bright_black,
                boxShadow:
                  "0 25px 50px -12px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.03)",
              }}
            >
              <div
                className="terminal-header"
                style={{
                  background: palette.black,
                  borderBottomColor: palette.bright_black,
                }}
              >
                <div className="terminal-dots">
                  <div className="terminal-dot" style={{ background: palette.red }} />
                  <div
                    className="terminal-dot"
                    style={{ background: palette.yellow }}
                  />
                  <div
                    className="terminal-dot"
                    style={{ background: palette.green }}
                  />
                </div>
                <span
                  className="terminal-header-title"
                  style={{ color: palette.foreground }}
                >
                  studio-preview
                </span>
              </div>
              <StudioTerminalPreview palette={palette} />
            </div>

            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Theme JSON</CardTitle>
              </CardHeader>
              <CardContent>
                <textarea
                  readOnly
                  value={schemaJson}
                  className="h-80 w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs"
                />
              </CardContent>
            </Card>
          </div>
        </div>
      </div>
    </section>
  );
}

function StudioTerminalPreview({
  palette,
}: {
  palette: ThemePalette;
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
      style={{ background: palette.background, color: palette.foreground }}
    >
      <div className="space-y-1">
        <div>
          <span style={{ color: palette.green }}>$</span> theme studio --preview
        </div>
        <div>
          <span style={{ color: palette.blue }}>→</span> validating palette...
        </div>
        <div>
          <span style={{ color: palette.magenta }}>→</span> ready to export
        </div>
      </div>

      <div className="mt-4 font-mono text-[12px] leading-tight">
        <div style={{ color: palette.red }}>error: sample error line</div>
        <div style={{ color: palette.yellow }}>warn: sample warning line</div>
        <div style={{ color: palette.green }}>ok: sample success line</div>
        <div style={{ color: palette.cyan }}>info: sample info line</div>
        <div style={{ color: palette.white }}>text: regular foreground text</div>
      </div>

      <div className="mt-4 space-y-1">
        {colorRows.map((row, rowIndex) => (
          <div key={rowIndex} className="flex">
            {row.map((color, colorIndex) => (
              <span
                key={colorIndex}
                className="inline-block h-5 w-8"
                style={{ backgroundColor: color }}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

function safeColorValue(value: string): string {
  const normalized = normalizeHex(value, fallbackPalette.foreground);
  return /^#[0-9a-fA-F]{6}$/.test(normalized)
    ? normalized
    : fallbackPalette.foreground;
}

function normalizeHex(value: string, fallback: string): string {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) {
    return trimmed.toLowerCase();
  }
  return fallback;
}
