import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { Release, Asset } from "@/hooks/useGitHubRelease";
import { classifyAssets, formatBytes } from "@/hooks/useGitHubRelease";

interface DownloadProps {
  release: Release | null;
  loading: boolean;
  error: string | null;
}

const platformIcons = {
  mac: (
    <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
      <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/>
    </svg>
  ),
  windows: (
    <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
      <path d="M3 12V6.75l6-1.32v6.48L3 12zm17-9v8.75l-10 .15V5.21L20 3zM3 13l6 .09v6.81l-6-1.15V13zm17 .25V22l-10-1.91V13.1l10 .15z"/>
    </svg>
  ),
  linux: (
    <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 448 512">
      <path d="M220.8 123.3c1 .5 1.8 1.7 3 1.7 1.1 0 2.8-.4 2.9-1.5.2-1.4-1.9-2.3-3.2-2.9-1.7-.7-3.9-1-5.5-.1-.4.2-.8.7-.6 1.1.3 1.3 2.3 1.1 3.4 1.7zm-21.9 1.7c1.2 0 2-1.2 3-1.7 1.1-.6 3.1-.4 3.5-1.6.2-.4-.2-.9-.6-1.1-1.6-.9-3.8-.6-5.5.1-1.3.6-3.4 1.5-3.2 2.9.1 1 1.8 1.5 2.8 1.4zM420 403.8c-3.6-4-5.3-11.6-7.2-19.7-1.8-8.1-3.9-16.8-10.5-22.4-1.3-1.1-2.6-2.1-4-2.9-1.3-.8-2.7-1.5-4.1-2 9.2-27.3 5.6-54.5-3.7-79.1-11.4-30.1-31.3-56.4-46.5-74.4-17.1-21.5-33.7-41.9-33.4-72C311.1 85.4 315.7.1 234.8 0 132.4-.2 158 103.4 156.9 135.2c-1.7 23.4-6.4 41.8-22.5 64.7-18.9 22.5-45.5 58.8-58.1 96.7-6 17.9-8.8 36.1-6.2 53.3-6.5 5.8-11.4 14.7-16.6 20.2-4.2 4.3-10.3 5.9-17 8.3s-14 6-18.5 14.5c-2.1 3.9-2.8 8.1-2.8 12.4 0 3.9.6 7.9 1.2 11.8 1.2 8.1 2.5 15.7.8 20.8-5.2 14.4-5.9 24.4-2.2 31.7 3.8 7.3 11.4 10.5 20.1 12.3 17.3 3.6 40.8 2.7 59.3 12.5 19.8 10.4 39.9 14.1 55.9 10.4 11.6-2.6 21.1-9.6 25.9-20.2 12.5-.1 26.3-5.4 48.3-6.6 14.9-1.2 33.6 5.3 55.1 4.1.6 2.3 1.4 4.6 2.5 6.7v.1c8.3 16.7 23.8 24.3 40.3 23 16.6-1.3 34.1-11 48.3-27.9 13.6-16.4 36-23.2 50.9-32.2 7.4-4.5 13.4-10.1 13.9-18.3.4-8.2-4.4-17.3-15.5-29.7zM223.7 87.3c9.8-22.2 34.2-21.8 44-.4 6.5 14.2 3.6 30.9-4.3 40.4-1.6-.8-5.9-2.6-12.6-4.9 1.1-1.2 3.1-2.7 3.9-4.6 4.8-11.8-.2-27-9.1-27.3-7.3-.5-13.9 10.8-11.8 23-4.1-2-9.4-3.5-13-4.4-1-6.9-.3-14.6 2.9-21.8zM183 75.8c10.1 0 20.8 14.2 19.1 33.5-3.5 1-7.1 2.5-10.2 4.6 1.2-8.9-3.3-20.1-9.6-19.6-8.4.7-9.8 21.2-1.8 28.1 1 .8 1.9-.2-5.9 5.5-15.6-14.6-10.5-52.1 8.4-52.1zm-13.6 60.7c6.2-4.6 13.6-10 14.1-10.5 4.7-4.4 13.5-14.2 27.9-14.2 7.1 0 15.6 2.3 25.9 8.9 6.3 4.1 11.3 4.4 22.6 9.3 8.4 3.5 13.7 9.7 10.5 18.2-2.6 7.1-11 14.4-22.7 18.1-11.1 3.6-19.8 16-38.2 14.9-3.9-.2-7-1-9.6-2.1-8-3.5-12.2-10.4-20-15-8.6-4.8-13.2-10.4-14.7-15.3-1.4-4.9 0-9 4.2-12.3zm3.3 334c-2.7 35.1-43.9 34.4-75.3 18-29.9-15.8-68.6-6.5-76.5-21.9-2.4-4.7-2.4-12.7 2.6-26.4v-.2c2.4-7.6.6-16-.6-23.9-1.2-7.8-1.8-15 .9-20 3.5-6.7 8.5-9.1 14.8-11.3 10.3-3.7 11.8-3.4 19.6-9.9 5.5-5.7 9.5-12.9 14.3-18 5.1-5.5 10-8.1 17.7-6.9 8.1 1.2 15.1 6.8 21.9 16l19.6 35.6c9.5 19.9 43.1 48.4 41 68.9zm-30.5-45.9c-6.5-5.1-12.3-11.4-12.3-11.4-16.1-18.7-28.1-48.2-38.4-59.6 6.6 1.3 13.1 4 17.2 8.8 5.5 6.7 11.7 24.1 18.8 32.7 5.5 6.7 11.4 12.5 15.4 23.1 1.1 3 1.4 6.7-.7 6.4zm23.5-45.7c1.7.9 3.4 2.3 5.4 4.2-4 .6-7.9.7-11.6.6 2.4-1.5 4.5-3.2 6.2-4.8zM288.8 255c-.7 3.9-6.9 34.4-25.3 50.3-24.2 21.1-42.6 31.3-54.5 26.4-10.2-4.1-13.8-18.6-9.2-34.4 4.2-14.4 12.9-27.3 26.5-38.8 17.5-15 37.3-24.1 52.7-24.1 5.4 0 10.2 1.2 13.3 4.3 4.5 4.5 1.3 20.9-3.5 16.3zm108.6 141.4c-1.3 6.1-9.1 9.8-17.6 13.4-12.3 5.4-25.5 9.5-35.3 22-5.8 7.4-11.4 17.6-17.9 26.1-6.5 8.5-14.3 15.3-24.1 16.4-10.8 1.2-22.3-4.5-29.3-17.6-1.3-2.4-2.3-5-3.1-7.6-4.3 1.1-9.4 1.4-15 .6-17.4-2.5-30.8-17.2-40.7-33.7-7.5-12.5-13.1-25.1-20.1-31.6-7.6-7-8.5-20.6.8-28.2 8.5-7 20.4-6.1 31.2 3.1 12.1 10.3 24.4 28.9 42.9 38.9 1.5 1.1 3.1 2 4.7 2.8-2.4-4-3.4-8.1-3.1-12.5.6-8.1 4.9-15.1 10.9-22.7 7.5-9.5 14.6-15 18.8-21.1 6.4-9.1 6-17.4 6-27.6 0-8.7.8-18.3 4.8-31.1l.5-1.5c.4-1.3.9-2.7 1.4-4 1-2.6 2.2-5.2 3.5-7.7 1.3-2.5 2.8-4.9 4.4-7.2 1.6-2.3 3.3-4.5 5.1-6.6 3.6-4.2 7.6-8 11.9-11.4 4.3-3.4 8.9-6.3 13.8-8.8 4.9-2.5 10-4.5 15.3-6.1 5.3-1.6 10.7-2.7 16.2-3.4 5.5-.7 11-1 16.4-.8 5.4.2 10.7.8 15.9 1.7 5.2.9 10.2 2.2 15.1 3.9 4.9 1.7 9.6 3.7 14.1 6.1 4.5 2.4 8.8 5.1 12.8 8.1 4 3 7.7 6.4 11.1 10 3.4 3.6 6.4 7.5 9.1 11.6-2.7-.5-5.4-.6-8.1-.4-13.4.7-24.5 5.3-36.1 11.5-12.6 6.8-25.1 16.4-33.2 30.4-8 13.5-10.5 32.3 7.8 52.3 1.7 1.8 3.5 3.6 5.4 5.2 6.7 5.8 14.1 10.6 22.1 14.3z"/>
    </svg>
  ),
};

function PlatformButton({
  platform,
  assets
}: {
  platform: "mac" | "windows" | "linux";
  assets: Asset[];
}) {
  if (assets.length === 0) return null;

  const labels = {
    mac: "macOS",
    windows: "Windows",
    linux: "Linux",
  };

  return (
    <a
      href={assets[0].browser_download_url}
      target="_blank"
      rel="noreferrer"
      className="flex items-center gap-3 p-4 rounded-xl border border-border/50 bg-card/50 hover:border-primary/50 hover:bg-card transition-all group"
    >
      <div className="flex items-center justify-center w-10 h-10 rounded-lg bg-secondary text-muted-foreground group-hover:bg-primary/10 group-hover:text-primary transition-colors">
        {platformIcons[platform]}
      </div>
      <div className="flex-1">
        <div className="font-medium text-foreground">{labels[platform]}</div>
        <div className="text-xs text-muted-foreground">{assets[0].name}</div>
      </div>
      <svg className="w-5 h-5 text-muted-foreground group-hover:text-primary transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
      </svg>
    </a>
  );
}

function AssetList({ title, assets }: { title: string; assets: Asset[] }) {
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
            className="flex items-center justify-between p-3 rounded-lg border border-border/50 hover:border-primary/30 bg-card/30 hover:bg-card/50 transition-all text-sm group"
          >
            <span className="font-mono text-muted-foreground group-hover:text-foreground transition-colors truncate">
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

export function Download({ release, loading, error }: DownloadProps) {
  const classified = release?.assets ? classifyAssets(release.assets) : null;

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  };

  return (
    <section id="download" className="py-24">
      <div className="text-center mb-16">
        <h2 className="text-3xl md:text-4xl font-bold mb-4">
          Get Termy
        </h2>
        <p className="text-muted-foreground">
          Available for all major platforms. Free and open source.
        </p>
      </div>

      {loading && (
        <div className="flex items-center justify-center py-12">
          <div className="animate-spin rounded-full h-8 w-8 border-2 border-primary border-t-transparent" />
        </div>
      )}

      {error && (
        <div className="max-w-md mx-auto p-4 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
          <p className="text-sm text-destructive">{error}</p>
        </div>
      )}

      {!loading && !error && release && (
        <div className="max-w-3xl mx-auto">
          {/* Version badge */}
          <div className="flex items-center justify-center gap-3 mb-8">
            <span className="px-3 py-1 rounded-full bg-primary/10 text-primary text-sm font-medium">
              {release.tag_name}
            </span>
            {release.published_at && (
              <span className="text-sm text-muted-foreground">
                {formatDate(release.published_at)}
              </span>
            )}
          </div>

          {/* Platform buttons */}
          <div className="grid gap-4 md:grid-cols-3 mb-12">
            <PlatformButton platform="mac" assets={classified?.mac ?? []} />
            <PlatformButton platform="windows" assets={classified?.windows ?? []} />
            <PlatformButton platform="linux" assets={classified?.linux ?? []} />
          </div>

          {/* Install via package manager */}
          <div className="mb-12 p-4 rounded-xl border border-border/50 bg-card/30">
            <p className="text-sm text-muted-foreground mb-3">
              Or install via package manager:
            </p>
            <Tabs defaultValue="homebrew" className="w-full">
              <TabsList variant="line" className="mb-3">
                <TabsTrigger value="homebrew">Homebrew</TabsTrigger>
                <TabsTrigger value="arch">Arch Linux</TabsTrigger>
              </TabsList>
              <TabsContent value="homebrew">
                <div className="space-y-2">
                  <div className="flex items-center gap-2 p-3 rounded-lg bg-background font-mono text-sm">
                    <code className="flex-1 text-primary">
                      brew tap lassejlv/termy https://github.com/lassejlv/termy
                    </code>
                  </div>
                  <div className="flex items-center gap-2 p-3 rounded-lg bg-background font-mono text-sm">
                    <code className="flex-1 text-primary">
                      brew install --cask termy
                    </code>
                  </div>
                </div>
              </TabsContent>
              <TabsContent value="arch">
                <div className="flex items-center gap-2 p-3 rounded-lg bg-background font-mono text-sm">
                  <code className="flex-1 text-primary">
                    paru -S termy-bin
                  </code>
                </div>
              </TabsContent>
            </Tabs>
          </div>

          {/* All downloads */}
          <details className="group">
            <summary className="flex items-center justify-center gap-2 text-sm text-muted-foreground cursor-pointer hover:text-foreground transition-colors">
              <span>View all downloads</span>
              <svg className="w-4 h-4 transition-transform group-open:rotate-180" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
              </svg>
            </summary>
            <div className="mt-6 grid gap-6 md:grid-cols-2 lg:grid-cols-3">
              <AssetList title="macOS" assets={classified?.mac ?? []} />
              <AssetList title="Windows" assets={classified?.windows ?? []} />
              <AssetList title="Linux" assets={classified?.linux ?? []} />
            </div>
          </details>

          {/* Release notes link */}
          <div className="mt-12 text-center">
            <Button variant="outline" asChild className="rounded-xl">
              <a href={release.html_url} target="_blank" rel="noreferrer">
                View release notes
                <svg className="w-4 h-4 ml-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
              </a>
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
