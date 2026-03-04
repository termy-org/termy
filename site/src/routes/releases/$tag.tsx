import { createFileRoute, Link } from "@tanstack/react-router";
import { useNotraChangelogById } from "@/hooks/useNotraReleases";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { formatDate, proseClasses } from "@/lib/utils";
import { ChevronLeft } from "lucide-react";
import Markdown from "react-markdown";

export const Route = createFileRoute("/releases/$tag")({
  component: ReleaseDetailPage,
});

function ReleaseDetailPage() {
  const { tag } = Route.useParams();
  const { data: post, isLoading, error } = useNotraChangelogById(tag);

  return (
    <section className="pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <Button asChild variant="ghost" size="sm" className="mb-8 text-muted-foreground hover:text-foreground">
          <Link to="/releases">
            <ChevronLeft className="w-4 h-4" />
            All changelogs
          </Link>
        </Button>

        {isLoading && <Spinner />}

        {error && (
          <div className="p-6 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
            <p className="text-destructive font-medium">Could not load changelog.</p>
            <Button asChild variant="ghost" size="sm" className="mt-4 text-muted-foreground hover:text-foreground">
              <Link to="/releases">View all changelogs</Link>
            </Button>
          </div>
        )}

        {!isLoading && !error && !post && (
          <div className="p-6 rounded-xl border border-border/50 bg-card/30 text-center">
            <p className="text-muted-foreground">Changelog not found.</p>
            <Button asChild variant="ghost" size="sm" className="mt-4 text-muted-foreground hover:text-foreground">
              <Link to="/releases">View all changelogs</Link>
            </Button>
          </div>
        )}

        {!isLoading && !error && post && (
          <>
            <div className="mb-8">
              <h1 className="text-4xl md:text-5xl font-bold mb-2">{post.title}</h1>
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
      </div>
    </section>
  );
}
