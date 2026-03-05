import { Check, Copy, Plus, X } from "lucide-react";
import { type JSX, useCallback, useEffect, useRef, useState } from "react";
import { InteractiveTerminal } from "@/components/InteractiveTerminal";
import { Button } from "@/components/ui/button";
import { getPreferredDownload, type Release } from "@/hooks/useGitHubRelease";

interface TerminalTab {
  id: number;
  title: string;
}

const installCommands = {
  homebrew:
    "brew tap lassejlv/termy https://github.com/lassejlv/termy && brew install --cask termy",
  arch: "paru -S termy-bin",
} as const;

type PackageManager = keyof typeof installCommands;

const pmLabels: Record<PackageManager, string> = {
  homebrew: "Homebrew",
  arch: "Arch Linux",
};

interface HeroProps {
  release: Release | null;
}

function getPackageManagerButtonClass(
  selectedPm: PackageManager,
  packageManager: PackageManager,
): string {
  const baseClassName =
    "px-2 py-1 rounded text-[11px] font-medium transition-colors";

  if (selectedPm === packageManager) {
    return `${baseClassName} bg-background text-foreground shadow-sm`;
  }

  return `${baseClassName} text-muted-foreground hover:text-foreground`;
}

const MAX_TABS = 3;

function TerminalWithTabs(): JSX.Element {
  const [tabs, setTabs] = useState<TerminalTab[]>([
    { id: 0, title: "termy" },
  ]);
  const [activeTabId, setActiveTabId] = useState(0);
  const nextTabIdRef = useRef(1);

  const addTab = useCallback(() => {
    if (tabs.length >= MAX_TABS) return;
    const id = nextTabIdRef.current++;
    setTabs((prev) => [...prev, { id, title: "~" }]);
    setActiveTabId(id);
  }, [tabs.length]);

  const closeTab = useCallback(
    (tabId: number) => {
      if (tabs.length <= 1) return;
      setTabs((prev) => {
        const updated = prev.filter((t) => t.id !== tabId);
        if (activeTabId === tabId) {
          const closedIndex = prev.findIndex((t) => t.id === tabId);
          const newActive =
            updated[Math.min(closedIndex, updated.length - 1)];
          setActiveTabId(newActive.id);
        }
        return updated;
      });
    },
    [tabs, activeTabId],
  );

  return (
    <div className="terminal-window">
      <div className="terminal-header">
        <div className="terminal-dots">
          <div className="terminal-dot bg-[#ff5f57]" />
          <div className="terminal-dot bg-[#febc2e]" />
          <div className="terminal-dot bg-[#28c840]" />
        </div>

        <div className="terminal-tabs">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={`terminal-tab ${activeTabId === tab.id ? "active" : ""}`}
              onClick={() => setActiveTabId(tab.id)}
            >
              <span className="terminal-tab-title">{tab.title}</span>
              {tabs.length > 1 && (
                <span
                  className="terminal-tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(tab.id);
                  }}
                >
                  <X size={10} />
                </span>
              )}
            </button>
          ))}
        </div>

        {tabs.length < MAX_TABS && (
          <button
            className="terminal-tab-add"
            onClick={addTab}
            aria-label="New tab"
          >
            <Plus size={14} />
          </button>
        )}
      </div>

      {tabs.map(
        (tab) =>
          activeTabId === tab.id && (
            <InteractiveTerminal key={tab.id} />
          ),
      )}
    </div>
  );
}

export function Hero({ release }: HeroProps): JSX.Element {
  const [pm, setPm] = useState<PackageManager>("homebrew");
  const [copied, setCopied] = useState(false);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout>>(null);
  const packageManagers = Object.keys(installCommands) as PackageManager[];

  useEffect(() => {
    return () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    };
  }, []);

  const preferredDownload = release?.assets
    ? getPreferredDownload(release.assets)
    : null;

  function handlePackageManagerChange(packageManager: PackageManager): void {
    setPm(packageManager);
    setCopied(false);
  }

  function handleCopy(): void {
    navigator.clipboard.writeText(installCommands[pm]);
    setCopied(true);
    if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    copyTimerRef.current = setTimeout(() => setCopied(false), 2000);
  }

  const pmButtons = packageManagers.map((key) => (
    <button
      key={key}
      onClick={() => handlePackageManagerChange(key)}
      className={getPackageManagerButtonClass(pm, key)}
    >
      {pmLabels[key]}
    </button>
  ));

  return (
    <section className="relative pt-20 sm:pt-28 pb-20">
      <div className="relative">
        {/* Headline */}
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
            className="mt-10 flex flex-col items-center gap-4 animate-blur-in"
            style={{ animationDelay: "300ms" }}
          >
            <div className="flex flex-wrap items-center justify-center gap-4">
              <Button
                size="lg"
                asChild
                className="font-medium hover:scale-[1.02] transition-transform"
              >
                <a
                  href={preferredDownload?.browser_download_url ?? "#download"}
                >
                  Get Termy
                </a>
              </Button>
              <Button
                variant="outline"
                size="lg"
                asChild
                className="font-medium"
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
            <div className="flex items-center gap-3 text-xs text-muted-foreground/50">
              <span className="flex items-center gap-1">
                <svg
                  className="w-3.5 h-3.5"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                >
                  <path d="M22 17.607c-.786 2.28-3.139 6.317-5.563 6.361-1.608.031-2.125-.953-3.963-.953-1.837 0-2.412.923-3.932.983-2.572.099-6.542-5.827-6.542-10.995 0-4.747 3.308-7.1 6.198-7.143 1.55-.028 3.014 1.045 3.959 1.045.949 0 2.727-1.29 4.596-1.101.782.033 2.979.315 4.389 2.377-3.741 2.442-3.158 7.549.858 9.426zm-5.222-17.607c-2.826.114-5.132 3.079-4.81 5.531 2.612.203 5.118-2.725 4.81-5.531z" />
                </svg>
                macOS
              </span>
              <span className="flex items-center gap-1">
                <img src="/linux-tux.svg" alt="Linux" className="w-3.5 h-3.5" />
                Linux
              </span>
            </div>
            <p className="text-sm text-muted-foreground">
              If you find Termy useful,{" "}
              <a
                href="https://github.com/sponsors/lassejlv"
                target="_blank"
                rel="noreferrer"
                className="underline underline-offset-4 hover:text-foreground transition-colors"
              >
                consider sponsoring
              </a>
              . It would help a lot.
            </p>
          </div>

          {/* Package manager install */}
          <div
            className="mt-6 mx-auto w-full max-w-xl animate-blur-in"
            style={{ animationDelay: "350ms" }}
          >
            <div className="rounded-lg border border-border/50 bg-secondary/50 overflow-hidden">
              <div className="flex items-center gap-1 px-3 py-1.5 shrink-0 border-b border-border/50 sm:hidden">
                {pmButtons}
              </div>
              <div className="flex items-center h-10">
                <div className="hidden sm:flex items-center gap-1 px-3 shrink-0 border-r border-border/50">
                  {pmButtons}
                </div>
                <code className="flex-1 px-3 font-mono text-xs text-primary truncate text-left">
                  {installCommands[pm]}
                </code>
                <button
                  onClick={handleCopy}
                  className="px-3 h-full text-muted-foreground hover:text-foreground transition-colors shrink-0 border-l border-border/50"
                  aria-label="Copy to clipboard"
                >
                  {copied ? (
                    <Check className="w-3.5 h-3.5 text-green-500" />
                  ) : (
                    <Copy className="w-3.5 h-3.5" />
                  )}
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* Terminal Preview */}
        <div
          className="mt-10 sm:mt-14 mx-auto max-w-5xl px-6 animate-blur-in"
          style={{ animationDelay: "450ms" }}
        >
          <TerminalWithTabs />
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
