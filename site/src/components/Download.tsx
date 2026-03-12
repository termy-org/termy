import { ChevronDown, ExternalLink, Info } from "lucide-react";
import type { JSX } from "react";
import { AppleIcon, LinuxIcon, WindowsIcon } from "@/components/platform-icons";
import { Spinner } from "@/components/ui/spinner";
import {
  classifyAssets,
  formatBytes,
  type Asset,
  type Release,
} from "@/hooks/useGitHubRelease";
import { formatDate } from "@/lib/utils";

interface DownloadProps {
  release: Release | null;
  loading: boolean;
  error: string | null;
}

interface Platform {
  key: "mac" | "windows" | "linux";
  label: string;
  icon: JSX.Element;
}

const platforms = [
  {
    key: "mac" as const,
    label: "macOS",
    icon: <AppleIcon />,
  },
  {
    key: "windows" as const,
    label: "Windows",
    icon: <WindowsIcon />,
  },
  {
    key: "linux" as const,
    label: "Linux",
    icon: <LinuxIcon imgClassName="w-5 h-5 grayscale brightness-200" />,
  },
] satisfies Platform[];

interface AssetListProps {
  title: string;
  assets: Asset[];
}

function getPrimaryAsset(assets: Asset[]): Asset | null {
  return assets[0] ?? null;
}

function AssetList({ title, assets }: AssetListProps): JSX.Element | null {
  if (assets.length === 0) return null;

  return (
    <div>
      <h4 className="text-sm font-medium text-muted-foreground mb-3">{title}</h4>
      <div className="space-y-2">
        {assets.map((asset) => (
          <a
            key={asset.name}
            href={asset.browser_download_url}
            target="_blank"
            rel="noreferrer"
            className="flex items-center justify-between p-3 rounded-lg border border-border/50 hover:border-primary/30 bg-card/50 transition-colors text-sm"
          >
            <span className="font-mono text-muted-foreground hover:text-foreground transition-colors truncate">
              {asset.name}
            </span>
            <span className="text-xs text-muted-foreground ml-4 shrink-0">
              {formatBytes(asset.size)}
            </span>
          </a>
        ))}
      </div>
    </div>
  );
}

export function Download({ release, loading, error }: DownloadProps): JSX.Element {
  const classified = release?.assets ? classifyAssets(release.assets) : null;

  return (
    <section id="download" className="py-24">
      <div
        className="text-center mb-12 animate-blur-in"
        style={{ animationDelay: "0ms" }}
      >
        <h2 className="text-3xl md:text-4xl font-bold tracking-tight">
          Ready to try Termy?
        </h2>
        <p className="mt-3 text-muted-foreground">
          Free and open source. Available for all major platforms.
        </p>
      </div>

      {loading && <Spinner />}

      {error && (
        <div className="max-w-md mx-auto p-4 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
          <p className="text-sm text-destructive">{error}</p>
        </div>
      )}

      {!loading && !error && release && (
        <div
          className="max-w-3xl mx-auto animate-blur-in"
          style={{ animationDelay: "100ms" }}
        >
          {/* Platform download cards - horizontal on desktop */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {platforms.map((platform) => {
              const assets = classified?.[platform.key] ?? [];
              const primary = getPrimaryAsset(assets);
              if (!primary) return null;

              return (
                <a
                  key={platform.key}
                  href={primary.browser_download_url}
                  target="_blank"
                  rel="noreferrer"
                  className="flex flex-col items-center gap-3 p-6 rounded-xl border border-border/50 bg-card/30 hover:border-primary/30 hover:bg-card/60 transition-all group"
                >
                  <span className="text-muted-foreground group-hover:text-primary transition-colors [&_svg]:w-8 [&_svg]:h-8 [&_img]:w-8 [&_img]:h-8">
                    {platform.icon}
                  </span>
                  <span className="font-medium text-foreground">
                    {platform.label}
                  </span>
                  <span className="font-mono text-[10px] text-muted-foreground/60 truncate max-w-full">
                    {primary.name}
                  </span>
                  <div className="flex items-center gap-1.5 text-xs text-primary opacity-0 group-hover:opacity-100 transition-opacity">
                    <svg
                      className="w-3.5 h-3.5"
                      fill="none"
                      viewBox="0 0 24 24"
                      strokeWidth={2}
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5M16.5 12L12 16.5m0 0L7.5 12m4.5 4.5V3"
                      />
                    </svg>
                    Download
                  </div>
                </a>
              );
            })}
          </div>

          {/* Version + meta */}
          <div className="mt-5 flex items-center justify-center gap-3 text-sm text-muted-foreground">
            <span className="font-mono text-xs px-2 py-0.5 rounded-md bg-primary/10 text-primary">
              {release.tag_name}
            </span>
            {release.published_at && (
              <span className="text-xs">{formatDate(release.published_at)}</span>
            )}
          </div>

          {/* Code signing note - collapsible */}
          <details className="mt-4 text-center max-w-md mx-auto group">
            <summary className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground/50 cursor-pointer hover:text-muted-foreground transition-colors">
              <Info className="w-3 h-3" />
              <span>Installation notes</span>
              <ChevronDown className="w-3 h-3 transition-transform group-open:rotate-180" />
            </summary>
            <p className="mt-3 text-xs text-muted-foreground/60 leading-relaxed">
              Termy is not code signed yet. On macOS, run{" "}
              <code className="rounded bg-secondary px-1 py-0.5 text-[10px] font-mono text-primary/80">
                sudo xattr -d com.apple.quarantine /Applications/Termy.app
              </code>{" "}
              if blocked. On Windows, click "More info" then "Run anyway".
            </p>
          </details>

          {/* Links */}
          <div className="mt-6 flex items-center justify-center gap-6">
            <details className="group">
              <summary className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors">
                All downloads
                <ChevronDown className="w-3 h-3 transition-transform group-open:rotate-180" />
              </summary>
              <div className="relative sm:absolute sm:left-1/2 sm:-translate-x-1/2 mt-4 w-full max-w-2xl grid gap-6 grid-cols-1 md:grid-cols-3 px-4">
                <AssetList title="macOS" assets={classified?.mac ?? []} />
                <AssetList title="Windows" assets={classified?.windows ?? []} />
                <AssetList title="Linux" assets={classified?.linux ?? []} />
              </div>
            </details>
            <a
              href={release.html_url}
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              Release notes
              <ExternalLink className="w-3 h-3" />
            </a>
          </div>
        </div>
      )}
    </section>
  );
}
