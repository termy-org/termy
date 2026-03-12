import { type JSX, useState } from "react";
import { InteractiveTerminal } from "@/components/InteractiveTerminal";
import { Button } from "@/components/ui/button";
import { getPreferredDownload, type Release } from "@/hooks/useGitHubRelease";
import { CopyButton } from "./animate-ui/components/buttons/copy";

type PackageManager = "brew" | "arch";

const installCommands: Record<PackageManager, string> = {
  brew: "brew tap lassejlv/termy https://github.com/lassejlv/termy && brew install --cask termy",
  arch: "paru -S termy-bin",
};

interface HeroProps {
  release: Release | null;
}

export function Hero({ release }: HeroProps): JSX.Element {
  const [packageManager, setPackageManager] = useState<PackageManager>("brew");
  const preferredDownload = release?.assets
    ? getPreferredDownload(release.assets)
    : null;

  const currentCommand = installCommands[packageManager];

  return (
    <section className="relative pt-20 sm:pt-28 pb-20">
      {/* Background grid pattern */}
      <div className="absolute inset-0 -z-10 overflow-hidden">
        <div className="absolute inset-0 bg-[linear-gradient(to_right,#8882_1px,transparent_1px),linear-gradient(to_bottom,#8882_1px,transparent_1px)] bg-[size:40px_40px] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_110%)]" />
      </div>
      <div className="relative">
        <div className="text-center max-w-4xl mx-auto px-6">
          <h1
            className="text-5xl md:text-7xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            A terminal that
            <br />
            <span className="gradient-text">gets out of your way.</span>
          </h1>

          <p
            className="mt-6 text-lg md:text-xl text-muted-foreground max-w-2xl mx-auto animate-blur-in"
            style={{ animationDelay: "150ms" }}
          >
            Lightweight, GPU-rendered, and built with Rust. Termy starts
            instantly and stays fast.
          </p>

          {/* CTAs */}
          <div
            className="mt-10 flex flex-wrap items-center justify-center gap-4 animate-blur-in"
            style={{ animationDelay: "300ms" }}
          >
            <Button
              size="lg"
              asChild
              className="font-medium hover:scale-[1.02] transition-transform"
            >
              <a href={preferredDownload?.browser_download_url ?? "#download"}>
                Get Termy
              </a>
            </Button>
            <Button variant="outline" size="lg" asChild className="font-medium">
              <a
                href="https://github.com/lassejlv/termy"
                target="_blank"
                rel="noreferrer"
              >
                <svg
                  className="w-5 h-5 mr-2"
                  fill="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    fillRule="evenodd"
                    clipRule="evenodd"
                    d="M12 2C6.477 2 2 6.477 2 12c0 4.42 2.87 8.17 6.84 9.5.5.08.66-.23.66-.5v-1.69c-2.77.6-3.36-1.34-3.36-1.34-.46-1.16-1.11-1.47-1.11-1.47-.91-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.87 1.52 2.34 1.07 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.92 0-1.11.38-2 1.03-2.71-.1-.25-.45-1.29.1-2.64 0 0 .84-.27 2.75 1.02.79-.22 1.65-.33 2.5-.33.85 0 1.71.11 2.5.33 1.91-1.29 2.75-1.02 2.75-1.02.55 1.35.2 2.39.1 2.64.65.71 1.03 1.6 1.03 2.71 0 3.82-2.34 4.66-4.57 4.91.36.31.69.92.69 1.85V21c0 .27.16.59.67.5C19.14 20.16 22 16.42 22 12A10 10 0 0012 2z"
                  />
                </svg>
                View on GitHub
              </a>
            </Button>
          </div>

          {/* Install command */}
          <div
            className="mt-6 mx-auto w-full max-w-2xl animate-blur-in"
            style={{ animationDelay: "350ms" }}
          >
            {/* Package manager tabs */}
            <div className="flex justify-center gap-1 mb-2">
              <button
                type="button"
                onClick={() => setPackageManager("brew")}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                  packageManager === "brew"
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                Homebrew
              </button>
              <button
                type="button"
                onClick={() => setPackageManager("arch")}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                  packageManager === "arch"
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                Arch Linux
              </button>
            </div>
            <div className="rounded-lg border border-border/50 bg-secondary/50 overflow-hidden">
              <div className="flex items-center h-10">
                <code className="flex-1 px-3 font-mono text-xs text-primary truncate text-left">
                  {currentCommand}
                </code>
                <CopyButton
                  className="px-3 h-full text-muted-foreground hover:text-foreground transition-colors shrink-0 border-l border-border/50"
                  variant="ghost"
                  content={currentCommand}
                />
              </div>
            </div>
          </div>
        </div>

        {/* Terminal Preview */}
        <div
          className="mt-10 sm:mt-14 mx-auto max-w-5xl px-6 animate-blur-in"
          style={{ animationDelay: "450ms" }}
        >
          <div className="terminal-window">
            <div className="terminal-header">
              <div className="terminal-dots">
                <div className="terminal-dot bg-[#ff5f57]" />
                <div className="terminal-dot bg-[#febc2e]" />
                <div className="terminal-dot bg-[#28c840]" />
              </div>
              <div className="terminal-tabs">
                <span className="terminal-tab active">
                  <span className="terminal-tab-title">termy</span>
                </span>
              </div>
            </div>
            <InteractiveTerminal />
          </div>
          <p
            className="mt-3 text-center text-xs text-muted-foreground/50 animate-blur-in"
            style={{ animationDelay: "600ms" }}
          >
            Try typing a command — it's interactive
          </p>
        </div>
      </div>
    </section>
  );
}
