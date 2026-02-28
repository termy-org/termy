import { createFileRoute, Link } from "@tanstack/react-router";
import { useGitHubReleases } from "@/hooks/useGitHubReleases";
import { classifyAssets } from "@/hooks/useGitHubRelease";

export const Route = createFileRoute("/releases/")({
  component: ReleasesPage,
});

function ReleasesPage() {
  const { releases, loading, error } = useGitHubReleases();

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  };

  return (
    <section className="pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <div className="mb-12">
          <Link
            to="/"
            className="text-sm text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1 mb-4"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
            </svg>
            Back to home
          </Link>
          <h1 className="text-4xl md:text-5xl font-bold">Releases</h1>
          <p className="mt-4 text-lg text-muted-foreground">
            All versions of Termy, from the latest to the oldest.
          </p>
        </div>

        {loading && (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin rounded-full h-8 w-8 border-2 border-primary border-t-transparent" />
          </div>
        )}

        {error && (
          <div className="p-4 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
            <p className="text-sm text-destructive">{error}</p>
          </div>
        )}

        {!loading && !error && (
          <div className="space-y-4">
            {releases.map((release, index) => {
              const classified = classifyAssets(release.assets);
              const totalAssets = classified.mac.length + classified.windows.length + classified.linux.length;

              return (
                <Link
                  key={release.tag_name}
                  to="/releases/$tag"
                  params={{ tag: release.tag_name }}
                  className="block p-6 rounded-xl border border-border/50 bg-card/30 hover:border-primary/50 hover:bg-card/50 transition-all group"
                >
                  <div className="flex flex-wrap items-start justify-between gap-4">
                    <div>
                      <div className="flex items-center gap-3">
                        <h2 className="text-xl font-semibold text-foreground group-hover:text-primary transition-colors">
                          {release.tag_name}
                        </h2>
                        {index === 0 && (
                          <span className="px-2 py-0.5 rounded-full bg-primary/10 text-primary text-xs font-medium">
                            Latest
                          </span>
                        )}
                      </div>
                      {release.published_at && (
                        <p className="mt-1 text-sm text-muted-foreground">
                          {formatDate(release.published_at)}
                        </p>
                      )}
                    </div>
                    <div className="flex items-center gap-4 text-sm text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        {totalAssets} assets
                      </span>
                      <svg className="w-5 h-5 text-muted-foreground group-hover:text-primary transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                      </svg>
                    </div>
                  </div>

                  {/* Platform badges */}
                  <div className="mt-4 flex flex-wrap gap-2">
                    {classified.mac.length > 0 && (
                      <span className="px-2 py-1 rounded-md bg-secondary text-xs text-secondary-foreground">
                        macOS
                      </span>
                    )}
                    {classified.windows.length > 0 && (
                      <span className="px-2 py-1 rounded-md bg-secondary text-xs text-secondary-foreground">
                        Windows
                      </span>
                    )}
                    {classified.linux.length > 0 && (
                      <span className="px-2 py-1 rounded-md bg-secondary text-xs text-secondary-foreground">
                        Linux
                      </span>
                    )}
                  </div>
                </Link>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
