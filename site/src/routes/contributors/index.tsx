import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import type { JSX } from "react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { ContributorCard } from "@/components/ContributorCard";
import { useContributors } from "@/hooks/useContributors";

export const Route = createFileRoute("/contributors/")({
  component: ContributorsPage,
});

function ContributorsPage(): JSX.Element {
  const { data: contributors, isLoading, error } = useContributors();
  const hasContributors = !isLoading && !error && Boolean(contributors);

  return (
    <section className="pt-24 sm:pt-32 pb-20">
      <div className="max-w-4xl mx-auto">
        <div className="mb-12">
          <Button asChild variant="ghost" size="sm" className="mb-4 text-muted-foreground hover:text-foreground">
            <Link to="/">
              <ChevronLeft className="w-4 h-4" />
              Back to home
            </Link>
          </Button>
          <h1 className="text-3xl sm:text-4xl md:text-5xl font-bold">Contributors</h1>
          <p className="mt-4 text-lg text-muted-foreground">
            The people who make Termy possible.
          </p>
        </div>

        {isLoading && <Spinner />}

        {error && (
          <div className="p-4 rounded-xl border border-destructive/50 bg-destructive/10 text-center">
            <p className="text-sm text-destructive">
              Could not load contributors. Try again in a moment.
            </p>
          </div>
        )}

        {hasContributors && contributors && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {contributors.map((contributor, index) => (
              contributor.author && (
                <ContributorCard
                  key={contributor.author.id}
                  contributor={contributor}
                  rank={index + 1}
                  index={index}
                />
              )
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
