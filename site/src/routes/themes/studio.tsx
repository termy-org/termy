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
    <section className="pt-24 pb-16">
      <div className="mx-auto max-w-6xl">
        {/* Header */}
        <div className="mb-6">
          <h1 className="text-3xl md:text-4xl font-bold">Theme Studio</h1>
          <p className="mt-2 text-muted-foreground">
            Build a theme and preview it live.
          </p>
        </div>

        <Button asChild variant="outline" className="mb-6">
          <Link to="/themes">Back to store</Link>
        </Button>

        {/* Content */}
        <div className="grid gap-6 lg:grid-cols-[320px_1fr]">
          <Card>
            <CardHeader>
              <CardTitle>Palette</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
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
                    className="mt-6 h-8 w-8 cursor-pointer rounded border border-border bg-background p-1"
                    aria-label={`Select ${field} color`}
                  />
                </label>
              ))}
            </CardContent>
          </Card>

          <div className="space-y-6">
            {/* Preview */}
            <div
              className="rounded-lg border overflow-hidden"
              style={{
                background: palette.background,
                borderColor: palette.bright_black,
              }}
            >
              <div
                className="flex items-center gap-2 px-4 py-2 border-b"
                style={{
                  background: palette.black,
                  borderColor: palette.bright_black,
                }}
              >
                <div
                  className="w-3 h-3 rounded-full"
                  style={{ background: palette.red }}
                />
                <div
                  className="w-3 h-3 rounded-full"
                  style={{ background: palette.yellow }}
                />
                <div
                  className="w-3 h-3 rounded-full"
                  style={{ background: palette.green }}
                />
              </div>
              <StudioTerminalPreview palette={palette} />
            </div>

            {/* JSON Output */}
            <Card>
              <CardHeader>
                <CardTitle>Theme JSON</CardTitle>
              </CardHeader>
              <CardContent>
                <textarea
                  readOnly
                  value={schemaJson}
                  className="h-64 w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-xs"
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
      className="p-4 font-mono text-sm"
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

      <div className="mt-4 space-y-1 text-xs">
        <div style={{ color: palette.red }}>error: sample error line</div>
        <div style={{ color: palette.yellow }}>warn: sample warning line</div>
        <div style={{ color: palette.green }}>ok: sample success line</div>
        <div style={{ color: palette.cyan }}>info: sample info line</div>
        <div style={{ color: palette.white }}>text: regular text</div>
      </div>

      <div className="mt-4 space-y-1">
        {colorRows.map((row, rowIndex) => (
          <div key={rowIndex} className="flex">
            {row.map((color, colorIndex) => (
              <span
                key={colorIndex}
                className="inline-block h-4 w-6"
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
