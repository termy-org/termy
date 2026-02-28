import { Button } from "@/components/ui/button";
import type { Release } from "@/hooks/useGitHubRelease";
import { getPreferredDownload } from "@/hooks/useGitHubRelease";

interface HeroProps {
  release: Release | null;
}

export function Hero({ release }: HeroProps) {
  const preferredDownload = release?.assets
    ? getPreferredDownload(release.assets)
    : null;

  return (
    <section className="relative pt-32 pb-20">
      <div className="relative">
        {/* Headline */}
        <div className="text-center max-w-4xl mx-auto px-6">
          <h1
            className="text-5xl md:text-7xl font-bold tracking-tight animate-fade-up"
            style={{ animationDelay: "100ms" }}
          >
            The <span className="gradient-text">minimal</span>
            <br />
            terminal emulator.
          </h1>

          <p
            className="mt-6 text-lg md:text-xl text-muted-foreground max-w-2xl mx-auto animate-fade-up"
            style={{ animationDelay: "200ms" }}
          >
            Blazingly fast terminal emulator built with Rust. GPU-accelerated
            rendering, instant startup, and beautiful by default.
          </p>

          {/* CTAs */}
          <div
            className="mt-10 flex flex-wrap items-center justify-center gap-4 animate-fade-up"
            style={{ animationDelay: "300ms" }}
          >
            <Button
              size="lg"
              asChild
              className="h-12 px-8 text-base font-medium rounded-xl glow-sm hover:scale-105 transition-transform"
            >
              <a href={preferredDownload?.browser_download_url ?? "#download"}>
                <svg
                  className="w-5 h-5 mr-2"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                  />
                </svg>
                Download for Free
              </a>
            </Button>
            <Button
              variant="outline"
              size="lg"
              asChild
              className="h-12 px-8 text-base font-medium rounded-xl border-border/80 text-foreground hover:bg-secondary/50 hover:border-primary/50 transition-colors"
            >
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
        </div>

        {/* Terminal Preview */}
        <div
          className="mt-20 mx-auto max-w-5xl px-6 animate-fade-up"
          style={{ animationDelay: "400ms" }}
        >
          <div className="terminal-window animate-float">
            <div className="terminal-header">
              <div className="terminal-dot bg-red-500" />
              <div className="terminal-dot bg-yellow-500" />
              <div className="terminal-dot bg-green-500" />
              <span className="ml-3 text-xs text-muted-foreground font-mono">
                termy
              </span>
            </div>
            <div className="relative">
              <img
                src="/termy-example.png"
                alt="Termy in action"
                className="w-full"
              />
              <div className="absolute inset-0 bg-gradient-to-t from-card/80 to-transparent pointer-events-none" />
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
