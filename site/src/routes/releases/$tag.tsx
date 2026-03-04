import { createFileRoute, Link } from "@tanstack/react-router";
import { useGitHubReleaseByTag } from "@/hooks/useGitHubReleases";
import { classifyAssets, formatBytes } from "@/hooks/useGitHubRelease";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { Asset } from "@/hooks/useGitHubRelease";
import Markdown from "react-markdown";
import { useState } from "react";

export const Route = createFileRoute("/releases/$tag")({
  component: ReleaseDetailPage,
});

function AssetList({ title, assets }: { title: string; assets: Asset[] }) {
  if (assets.length === 0) return null;

  return (
    <div className="p-4 rounded-xl border border-border/50 bg-card/30">
      <h3 className="text-sm font-semibold text-foreground mb-3">{title}</h3>
      <div className="space-y-2">
        {assets.map((asset) => (
          <a
            key={asset.name}
            href={asset.browser_download_url}
            target="_blank"
            rel="noreferrer"
            className="flex items-center justify-between p-3 rounded-lg border border-border/50 hover:border-primary/30 bg-background/50 hover:bg-background transition-all text-sm group"
          >
            <div className="flex items-center gap-3">
              <svg
                className="w-5 h-5 text-muted-foreground group-hover:text-primary transition-colors"
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
              <span className="font-mono text-foreground group-hover:text-primary transition-colors">
                {asset.name}
              </span>
            </div>
            <span className="text-xs text-muted-foreground">
              {formatBytes(asset.size)}
            </span>
          </a>
        ))}
      </div>
    </div>
  );
}

function ReleaseDetailPage() {
  const { tag } = Route.useParams();
  const { release, loading, error } = useGitHubReleaseByTag(tag);
  const [showAssets, setShowAssets] = useState(false);
  const [showPackageManager, setShowPackageManager] = useState(false);

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  };

  const classified = release?.assets ? classifyAssets(release.assets) : null;

  return (
    <section className="pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <Link
          to="/releases"
          className="text-sm text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1 mb-8"
        >
          <svg
            className="w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M15 19l-7-7 7-7"
            />
          </svg>
          All releases
        </Link>

        {loading && (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-8 w-8 border-2 border-primary border-t-transparent" />
          </div>
        )}

        {error && (
          <div className="p-6 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
            <p className="text-destructive font-medium">{error}</p>
            <Link
              to="/releases"
              className="mt-4 inline-block text-sm text-muted-foreground hover:text-foreground"
            >
              View all releases
            </Link>
          </div>
        )}

        {!loading && !error && release && (
          <>
            {/* Header */}
            <div className="mb-8">
              <div className="flex flex-wrap items-center gap-3 mb-2">
                <h1 className="text-4xl md:text-5xl font-bold">
                  {release.tag_name}
                </h1>
              </div>
              {release.published_at && (
                <p className="text-muted-foreground">
                  Released on {formatDate(release.published_at)}
                </p>
              )}
            </div>

            {/* Quick actions */}
            <div className="flex flex-wrap gap-3 mb-8">
              <Button asChild>
                <a href={release.html_url} target="_blank" rel="noreferrer">
                  <svg
                    className="w-4 h-4 mr-2"
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

            {/* Release Notes */}
            {release.body && (
              <div className="mb-8">
                <h2 className="text-xl font-semibold mb-4">Release Notes</h2>
                <div className="p-6 rounded-xl border border-border/50 bg-card/30 prose prose-sm prose-invert max-w-none prose-headings:text-foreground prose-p:text-muted-foreground prose-strong:text-foreground prose-a:text-primary prose-a:no-underline hover:prose-a:underline prose-code:text-primary prose-code:bg-secondary prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded prose-code:before:content-none prose-code:after:content-none prose-pre:bg-background prose-pre:border prose-pre:border-border/50 prose-ul:text-muted-foreground prose-li:marker:text-muted-foreground">
                  <Markdown>{release.body}</Markdown>
                </div>
              </div>
            )}

            {/* Downloads */}
            <div className="mb-8">
              <div className="mb-4 flex items-center justify-between gap-3">
                <h2 className="text-xl font-semibold">Downloads</h2>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setShowAssets((open) => !open)}
                  aria-expanded={showAssets}
                  aria-controls="release-assets"
                >
                  {showAssets ? "Hide assets" : "Show assets"}
                  <svg
                    className={`h-4 w-4 transition-transform ${showAssets ? "rotate-180" : ""}`}
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M19 9l-7 7-7-7"
                    />
                  </svg>
                </Button>
              </div>

              <div
                id="release-assets"
                className={`grid gap-4 transition-all duration-200 md:grid-cols-2 ${
                  showAssets
                    ? "mb-4 max-h-[1000px] opacity-100"
                    : "max-h-0 overflow-hidden opacity-0"
                }`}
              >
                <AssetList title="macOS" assets={classified?.mac ?? []} />
                <AssetList title="Windows" assets={classified?.windows ?? []} />
                <AssetList title="Linux" assets={classified?.linux ?? []} />
              </div>

              <div className="mb-4 rounded-xl border border-amber-500/30 bg-amber-500/10 p-4">
                <p className="mb-2 text-sm text-amber-200">
                  Termy is not code signed yet.
                </p>
                <p className="text-sm text-amber-100/90">
                  On macOS, if Gatekeeper blocks launch after moving Termy to
                  Applications, run{" "}
                  <code className="rounded bg-background px-1.5 py-0.5 text-primary">
                    sudo xattr -d com.apple.quarantine /Applications/Termy.app
                  </code>
                  .
                </p>
                <p className="mt-2 text-sm text-amber-100/90">
                  On Windows, click <strong>More info</strong> and then{" "}
                  <strong>Run anyway</strong> in the SmartScreen prompt.
                </p>
              </div>

              <div className="mt-4 rounded-xl border border-border/50 bg-card/30 p-4">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <p className="text-sm text-muted-foreground">
                    Or install via package manager:
                  </p>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setShowPackageManager((open) => !open)}
                    aria-expanded={showPackageManager}
                    aria-controls="package-manager-install"
                  >
                    {showPackageManager ? "Hide commands" : "Show commands"}
                    <svg
                      className={`h-4 w-4 transition-transform ${showPackageManager ? "rotate-180" : ""}`}
                      fill="none"
                      viewBox="0 0 24 24"
                      stroke="currentColor"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M19 9l-7 7-7-7"
                      />
                    </svg>
                  </Button>
                </div>

                <div
                  id="package-manager-install"
                  className={`transition-all duration-200 ${
                    showPackageManager
                      ? "max-h-[600px] opacity-100"
                      : "max-h-0 overflow-hidden opacity-0"
                  }`}
                >
                  <Tabs defaultValue="homebrew" className="w-full">
                    <TabsList variant="line" className="mb-3">
                      <TabsTrigger value="homebrew">Homebrew</TabsTrigger>
                      <TabsTrigger value="arch">Arch Linux</TabsTrigger>
                    </TabsList>
                    <TabsContent value="homebrew">
                      <div className="space-y-2">
                        <div className="flex items-center gap-2 rounded-lg bg-background p-3 font-mono text-sm">
                          <code className="flex-1 text-primary">
                            brew tap lassejlv/termy
                            https://github.com/lassejlv/termy
                          </code>
                        </div>
                        <div className="flex items-center gap-2 rounded-lg bg-background p-3 font-mono text-sm">
                          <code className="flex-1 text-primary">
                            brew install --cask termy
                          </code>
                        </div>
                      </div>
                    </TabsContent>
                    <TabsContent value="arch">
                      <div className="flex items-center gap-2 rounded-lg bg-background p-3 font-mono text-sm">
                        <code className="flex-1 text-primary">
                          paru -S termy-bin
                        </code>
                      </div>
                    </TabsContent>
                  </Tabs>
                </div>
              </div>
            </div>

            {/* Stats */}
            <div className="text-sm text-muted-foreground">
              <p>
                Total assets: {release.assets.length} |{" "}
                {classified && (
                  <>
                    macOS: {classified.mac.length} | Windows:{" "}
                    {classified.windows.length} | Linux:{" "}
                    {classified.linux.length}
                  </>
                )}
              </p>
            </div>
          </>
        )}
      </div>
    </section>
  );
}
