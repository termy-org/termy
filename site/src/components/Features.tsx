import type { JSX, ReactNode } from "react";
import { AppleIcon, LinuxIcon, WindowsIcon } from "@/components/platform-icons";

interface Feature {
  tag: string;
  title: string;
  description: string;
  mockup: ReactNode;
}

const features: Feature[] = [
  {
    tag: "Performance",
    title: "GPU-accelerated rendering.",
    description:
      "Instant startup, zero lag. Every frame rendered on the GPU for buttery smooth scrolling.",
    mockup: (
      <div className="font-mono text-[11px] leading-tight text-muted-foreground/70 space-y-2">
        <div className="text-primary/60">$ termy --benchmark</div>
        <div className="space-y-2.5 pt-1">
          {[
            { label: "Startup", value: "8ms", width: "15%" },
            { label: "Render", value: "2ms", width: "10%" },
            { label: "Memory", value: "18MB", width: "20%" },
          ].map((item) => (
            <div key={item.label} className="flex items-center gap-3">
              <span className="text-foreground/50 w-12 text-right text-[10px]">
                {item.label}
              </span>
              <span className="flex-1 h-2 rounded-full bg-primary/10 overflow-hidden">
                <span
                  className="block h-full bg-primary rounded-full"
                  style={{ width: item.width }}
                />
              </span>
              <span className="text-primary text-[10px] w-8 font-medium">
                {item.value}
              </span>
            </div>
          ))}
        </div>
        <div className="text-primary/40 pt-1 text-[10px]">
          All benchmarks passed ✓
        </div>
      </div>
    ),
  },
  {
    tag: "Config",
    title: "One file. Full control.",
    description:
      "Theme, font, keybindings — everything lives in a single TOML file. No hidden settings.",
    mockup: (
      <div className="font-mono text-[11px] leading-relaxed text-muted-foreground/70">
        <div className="text-primary/30 text-[10px]">
          # ~/.config/termy/config.toml
        </div>
        <div className="mt-2 space-y-1.5">
          <div>
            <span className="text-primary/60">[font]</span>
          </div>
          <div className="pl-2">
            family = <span className="text-primary">"JetBrains Mono"</span>
          </div>
          <div className="pl-2">
            size = <span className="text-primary">14</span>
          </div>
          <div className="mt-1">
            <span className="text-primary/60">[theme]</span>
          </div>
          <div className="pl-2">
            name = <span className="text-primary">"catppuccin"</span>
          </div>
          <div className="mt-1">
            <span className="text-primary/60">[window]</span>
          </div>
          <div className="pl-2">
            opacity = <span className="text-primary">0.95</span>
          </div>
          <div className="pl-2">
            blur = <span className="text-primary">true</span>
          </div>
        </div>
      </div>
    ),
  },
  {
    tag: "Platform",
    title: "Runs everywhere you do.",
    description:
      "Native on macOS, Windows, and Linux. Same config, same experience across all your machines.",
    mockup: (
      <div className="flex items-center justify-center h-full gap-8">
        {/* macOS */}
        <div className="flex flex-col items-center gap-2">
          <AppleIcon className="w-9 h-9 text-muted-foreground/40" />
          <span className="text-[10px] text-muted-foreground/50 font-medium">
            macOS
          </span>
        </div>
        {/* Windows */}
        <div className="flex flex-col items-center gap-2">
          <WindowsIcon className="w-9 h-9 text-muted-foreground/40" />
          <span className="text-[10px] text-muted-foreground/50 font-medium">
            Windows
          </span>
        </div>
        {/* Linux */}
        <div className="flex flex-col items-center gap-2">
          <LinuxIcon imgClassName="w-9 h-9 opacity-40 grayscale" />
          <span className="text-[10px] text-muted-foreground/50 font-medium">
            Linux
          </span>
        </div>
      </div>
    ),
  },
  {
    tag: "Themes",
    title: "Make it yours.",
    description:
      "Ship with built-in themes. Or go custom — full control over every color.",
    mockup: (
      <div className="space-y-3">
        <div className="grid grid-cols-4 gap-2">
          {(
            [
              ["#1e1e2e", "#cdd6f4", "#89b4fa", "#a6e3a1"],
              ["#282828", "#ebdbb2", "#fabd2f", "#b8bb26"],
              ["#24283b", "#c0caf5", "#7aa2f7", "#9ece6a"],
              ["#0d1117", "#c9d1d9", "#58a6ff", "#3fb950"],
            ] as const
          ).map((colors, i) => (
            <div
              key={i}
              className="rounded-lg border border-border/20 overflow-hidden shadow-sm"
            >
              <div
                className="p-2 space-y-1"
                style={{ background: colors[0] }}
              >
                <div
                  className="h-[3px] w-4/5 rounded-full"
                  style={{ background: colors[1], opacity: 0.25 }}
                />
                <div className="flex gap-1">
                  <div
                    className="h-[3px] w-3 rounded-full"
                    style={{ background: colors[2], opacity: 0.6 }}
                  />
                  <div
                    className="h-[3px] w-5 rounded-full"
                    style={{ background: colors[1], opacity: 0.15 }}
                  />
                </div>
                <div className="flex gap-1">
                  <div
                    className="h-[3px] w-4 rounded-full"
                    style={{ background: colors[3], opacity: 0.6 }}
                  />
                  <div
                    className="h-[3px] w-2 rounded-full"
                    style={{ background: colors[1], opacity: 0.15 }}
                  />
                </div>
              </div>
            </div>
          ))}
        </div>
        <div className="font-mono text-[9px] text-muted-foreground/40 text-center tracking-wider">
          catppuccin · gruvbox · tokyonight · github
        </div>
      </div>
    ),
  },
  {
    tag: "Efficiency",
    title: "Light on resources. Heavy on features.",
    description:
      "Minimal memory footprint. Built with Rust for maximum efficiency and reliability.",
    mockup: (
      <div className="font-mono text-[11px] leading-tight text-muted-foreground/70 space-y-3">
        <div className="text-primary/40 text-[10px]">$ btm --termy</div>
        <div className="space-y-3">
          <div className="space-y-1">
            <div className="flex justify-between text-[10px]">
              <span className="font-medium text-foreground/60">termy</span>
              <span className="text-primary font-medium">18 MB</span>
            </div>
            <div className="h-2 rounded-full bg-primary/10 overflow-hidden">
              <div className="h-full w-[6%] bg-primary rounded-full" />
            </div>
          </div>
          <div className="space-y-1">
            <div className="flex justify-between text-[10px]">
              <span className="text-muted-foreground/35">electron-term</span>
              <span className="text-muted-foreground/35">285 MB</span>
            </div>
            <div className="h-2 rounded-full bg-muted/40 overflow-hidden">
              <div className="h-full w-[85%] bg-muted-foreground/15 rounded-full" />
            </div>
          </div>
          <div className="space-y-1">
            <div className="flex justify-between text-[10px]">
              <span className="text-muted-foreground/35">other-term</span>
              <span className="text-muted-foreground/35">142 MB</span>
            </div>
            <div className="h-2 rounded-full bg-muted/40 overflow-hidden">
              <div className="h-full w-[45%] bg-muted-foreground/15 rounded-full" />
            </div>
          </div>
        </div>
      </div>
    ),
  },
  {
    tag: "Developer",
    title: "Built for the terminal power user.",
    description:
      "Splits, tabs, multiplexing, and shell integration. Everything flows.",
    mockup: (
      <div className="font-mono text-[11px] leading-tight">
        <div className="rounded-lg border border-border/30 overflow-hidden">
          <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-muted/20 border-b border-border/20">
            <div className="flex gap-1">
              <div className="w-2 h-2 rounded-full bg-red-400/30" />
              <div className="w-2 h-2 rounded-full bg-yellow-400/30" />
              <div className="w-2 h-2 rounded-full bg-green-400/30" />
            </div>
            <div className="flex gap-3 ml-2 text-[9px]">
              <span className="text-primary/60 border-b border-primary/40 pb-px">
                zsh
              </span>
              <span className="text-muted-foreground/25">node</span>
              <span className="text-muted-foreground/25">vim</span>
            </div>
          </div>
          <div className="flex divide-x divide-border/20">
            <div className="flex-1 p-2.5 space-y-1 text-muted-foreground/45">
              <div>
                <span className="text-primary/50">~</span> git log --oneline
              </div>
              <div className="text-[9px] pl-1">a1b2c3d refactor: cleanup</div>
              <div className="text-[9px] pl-1">e4f5g6h feat: add splits</div>
            </div>
            <div className="flex-1 p-2.5 space-y-1 text-muted-foreground/45">
              <div>
                <span className="text-primary/50">~</span> cargo test
              </div>
              <div className="text-primary/35 text-[9px] pl-1">
                running 42 tests...
              </div>
              <div className="text-primary/35 text-[9px] pl-1">
                42 passed ✓
              </div>
            </div>
          </div>
        </div>
      </div>
    ),
  },
];

function FeatureCard({
  feature,
  index,
}: {
  feature: Feature;
  index: number;
}): JSX.Element {
  return (
    <div
      className="animate-blur-in group flex flex-col rounded-xl border border-border/40 bg-card/30 transition-all duration-300 hover:border-primary/20 hover:bg-card/60 overflow-hidden"
      style={{ animationDelay: `${(index + 1) * 100}ms` }}
    >
      {/* Mockup area — fixed height so text rows align */}
      <div className="h-[180px] sm:h-[200px] p-4 sm:p-5 flex items-center">
        <div className="w-full">{feature.mockup}</div>
      </div>

      {/* Divider */}
      <div className="mx-4 sm:mx-5 border-t border-border/30" />

      {/* Text content — pushed to bottom with mt-auto so it aligns across the row */}
      <div className="p-4 sm:p-5 mt-auto flex flex-col gap-2">
        <h3 className="text-[15px] font-semibold text-foreground leading-tight">
          {feature.title}
        </h3>
        <p className="text-sm text-muted-foreground/80 leading-relaxed">
          {feature.description}
        </p>
        <span className="text-[11px] text-primary/50 font-mono tracking-wide mt-1">
          {feature.tag}
        </span>
      </div>
    </div>
  );
}

export function Features(): JSX.Element {
  return (
    <section id="features" className="py-24">
      <div
        className="text-center mb-16 animate-blur-in"
        style={{ animationDelay: "0ms" }}
      >
        <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
          Everything you need.
          <br />
          <span className="text-muted-foreground">Nothing you don't.</span>
        </h2>
      </div>

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {features.map((feature, i) => (
          <FeatureCard key={feature.tag} feature={feature} index={i} />
        ))}
      </div>
    </section>
  );
}
