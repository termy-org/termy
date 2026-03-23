import { Link, createFileRoute } from "@tanstack/react-router";
import type { JSX } from "react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { type ThemePalette, fallbackPalette } from "@/lib/theme-store";

export const Route = createFileRoute("/themes/studio")({
  component: ThemeStudioPage,
});

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
] as const satisfies readonly (keyof Required<ThemePalette>)[];

function ThemeStudioPage(): JSX.Element {
  const [palette, setPalette] =
    useState<Required<ThemePalette>>(fallbackPalette);

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

  function handleColorChange(
    key: keyof Required<ThemePalette>,
    value: string,
  ): void {
    setPalette((previous) => ({
      ...previous,
      [key]: normalizeHex(value, previous[key]),
    }));
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-6xl space-y-6">
        {/* Hero */}
        <div className="text-center max-w-3xl mx-auto px-6">
          <h1
            className="text-4xl md:text-6xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            Build a theme{" "}
            <span className="gradient-text">live.</span>
          </h1>
          <p
            className="mt-4 text-lg text-muted-foreground animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            Tune every color and see the terminal preview update instantly.
          </p>
          <div
            className="mt-6 flex flex-wrap items-center justify-center gap-3 animate-blur-in"
            style={{ animationDelay: "200ms" }}
          >
            <Button asChild variant="outline">
              <Link to="/themes">Back to store</Link>
            </Button>
          </div>
        </div>

        {/* Content grid */}
        <div
          className="grid gap-6 lg:grid-cols-[380px_minmax(0,1fr)] animate-blur-in"
          style={{ animationDelay: "300ms" }}
        >
          <Card className="border-border/60">
            <CardHeader>
              <CardTitle>Palette</CardTitle>
            </CardHeader>
            <CardContent className="max-h-[400px] sm:max-h-[700px] space-y-3 overflow-auto pr-1">
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
                    onChange={(event) =>
                      handleColorChange(field, event.target.value)
                    }
                    className="mt-6 h-8 w-8 sm:h-10 sm:w-10 cursor-pointer rounded border border-border bg-background p-1"
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
                  <div
                    className="terminal-dot"
                    style={{ background: palette.red }}
                  />
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
  palette: Required<ThemePalette>;
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
