import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import type { JSX } from "react";
import Markdown from "react-markdown";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useNotraChangelogById } from "@/hooks/useNotraReleases";
import { formatDate, proseClasses } from "@/lib/utils";

export const Route = createFileRoute("/releases/$tag")({
  component: ReleaseDetailPage,
});

interface EmptyStateProps {
  message: string;
  tone: "destructive" | "neutral";
}

function EmptyState({ message, tone }: EmptyStateProps): JSX.Element {
  const containerClassName =
    tone === "destructive"
      ? "p-6 rounded-xl border border-destructive/50 bg-destructive/10 text-center"
      : "p-6 rounded-xl border border-border/50 bg-card/30 text-center";

  const messageClassName =
    tone === "destructive"
      ? "text-destructive font-medium"
      : "text-muted-foreground";

  return (
    <div className={containerClassName}>
      <p className={messageClassName}>{message}</p>
      <Button
        asChild
        variant="ghost"
        size="sm"
        className="mt-4 text-muted-foreground hover:text-foreground"
      >
        <Link to="/releases">View all changelogs</Link>
      </Button>
    </div>
  );
}

function ReleaseDetailPage(): JSX.Element {
  const { tag } = Route.useParams();
  const { data: post, isLoading, error } = useNotraChangelogById(tag);

  return (
    <section className="pt-24 sm:pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <Button
          asChild
          variant="ghost"
          size="sm"
          className="mb-8 text-muted-foreground hover:text-foreground"
        >
          <Link to="/releases">
            <ChevronLeft className="w-4 h-4" />
            All changelogs
          </Link>
        </Button>

        {isLoading && <Spinner />}

        {error && (
          <EmptyState message="Could not load changelog." tone="destructive" />
        )}

        {!isLoading && !error && !post && (
          <EmptyState message="Changelog not found." tone="neutral" />
        )}

        {!isLoading && !error && post && (
          <>
            <div className="mb-8">
              <h1 className="text-3xl sm:text-4xl md:text-5xl font-bold mb-2">
                {post.title}
              </h1>
              <time className="text-muted-foreground" dateTime={post.createdAt}>
                {formatDate(post.createdAt)}
              </time>
            </div>

            <div className={`mb-8 ${proseClasses}`}>
              <Markdown>{post.markdown}</Markdown>
            </div>

            <Button asChild variant="outline" size="default">
              <Link to="/releases">
                <ChevronLeft className="w-4 h-4" />
                Back to all changelogs
              </Link>
            </Button>
          </>
        )}

        <div className="mt-16 pt-6 border-t border-border/50 flex items-center justify-center gap-2 text-sm text-muted-foreground">
          <span>Powered by Notra</span>
          <img src="/notra.svg" alt="Notra" className="h-5 w-5" />
        </div>
      </div>
    </section>
  );
}
