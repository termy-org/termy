import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import type { JSX } from "react";
import Markdown from "react-markdown";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useNotraChangelogs } from "@/hooks/useNotraReleases";
import { formatDate, proseClasses } from "@/lib/utils";

export const Route = createFileRoute("/releases/")({
  component: ReleasesPage,
});

function ReleasesPage(): JSX.Element {
  const { data: posts, isLoading, error } = useNotraChangelogs();
  const hasPosts = !isLoading && !error && Boolean(posts);

  return (
    <section className="pt-24 sm:pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <div className="mb-12">
          <Button
            asChild
            variant="ghost"
            size="sm"
            className="mb-4 text-muted-foreground hover:text-foreground"
          >
            <Link to="/">
              <ChevronLeft className="w-4 h-4" />
              Back to home
            </Link>
          </Button>
          <h1 className="text-3xl sm:text-4xl md:text-5xl font-bold">
            Changelog
          </h1>
          <p className="mt-4 text-lg text-muted-foreground">
            All updates to Termy, from the latest to the oldest.
          </p>
        </div>

        {isLoading && <Spinner />}

        {error && (
          <div className="p-4 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
            <p className="text-sm text-destructive">
              Could not load changelogs. Try again in a moment.
            </p>
          </div>
        )}

        {hasPosts && posts && (
          <div className="space-y-12">
            {posts.map((post, index) => (
              <article key={post.id}>
                <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 mb-4">
                  <Link
                    to="/releases/$tag"
                    params={{ tag: post.id }}
                    className="text-2xl font-bold text-foreground hover:text-primary transition-colors"
                  >
                    {post.title}
                  </Link>
                  <time
                    className="text-sm text-muted-foreground"
                    dateTime={post.createdAt}
                  >
                    {formatDate(post.createdAt)}
                  </time>
                </div>

                <div className={proseClasses}>
                  <Markdown>{post.markdown}</Markdown>
                </div>

                {index < posts.length - 1 && (
                  <hr className="mt-12 border-border/50" />
                )}
              </article>
            ))}
          </div>
        )}

        <div className="mt-16 pt-6 border-t border-border/50 flex items-center justify-center gap-2 text-sm text-muted-foreground">
          <span>Powered by Notra</span>
          <img src="/notra.svg" alt="Notra" className="h-5 w-5" />
        </div>
      </div>
    </section>
  );
}
