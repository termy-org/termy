import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { getDocsByCategory, sortDocCategories } from "@/lib/docs";
import { Sidebar } from "@/components/docs/Sidebar";
import { useCallback } from "react";

type SearchParams = { q?: string };

export const Route = createFileRoute("/docs/")({
  component: DocsPage,
  validateSearch: (search: Record<string, unknown>): SearchParams => ({
    q: typeof search.q === "string" ? search.q : undefined,
  }),
});

function DocsPage() {
  const { q: search = "" } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const docsByCategory = getDocsByCategory();
  const categories = sortDocCategories(Object.keys(docsByCategory));

  const handleSearchChange = useCallback(
    (value: string) => {
      navigate({
        search: value ? { q: value } : {},
        replace: true,
      });
    },
    [navigate],
  );

  return (
    <section className="pt-24 pb-20">
      <div className="flex gap-8">
        {/* Sidebar - hidden on mobile, visible on desktop */}
        <Sidebar
          currentSlug=""
          search={search}
          onSearchChange={handleSearchChange}
        />

        {/* Main content */}
        <main className="flex-1 min-w-0">
          {/* Mobile back link */}
          <Link
            to="/"
            className="lg:hidden text-sm text-muted-foreground hover:text-foreground transition-colors inline-flex items-center gap-1 mb-6"
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
            Back to home
          </Link>

          <div className="mb-8">
            <h1 className="text-3xl md:text-4xl font-bold">Documentation</h1>
            <p className="mt-3 text-muted-foreground">
              Step-by-step guides for installing, configuring, and
              troubleshooting Termy.
            </p>
          </div>

          <div className="mb-10 rounded-xl border border-border/50 bg-card/30 p-5">
            <h2 className="text-lg font-semibold text-foreground mb-3">
              Start Here
            </h2>
            <div className="grid gap-3 sm:grid-cols-3">
              <Link
                to="/docs/$"
                params={{ _splat: "installation" }}
                className="rounded-lg border border-border/50 bg-background/50 px-4 py-3 text-sm text-foreground hover:border-primary/40 transition-colors"
              >
                Install Termy
              </Link>
              <Link
                to="/docs/$"
                params={{ _splat: "first-steps" }}
                className="rounded-lg border border-border/50 bg-background/50 px-4 py-3 text-sm text-foreground hover:border-primary/40 transition-colors"
              >
                First Steps
              </Link>
              <Link
                to="/docs/$"
                params={{ _splat: "troubleshooting" }}
                className="rounded-lg border border-border/50 bg-background/50 px-4 py-3 text-sm text-foreground hover:border-primary/40 transition-colors"
              >
                Troubleshooting
              </Link>
            </div>
          </div>

          <div className="space-y-8">
            {categories.map((category) => (
              <div key={category}>
                <h2 className="text-xl font-semibold mb-4 text-foreground">
                  {category}
                </h2>
                <div className="grid gap-3 sm:grid-cols-2">
                  {docsByCategory[category].map((doc) => (
                    <Link
                      key={doc.slug}
                      to="/docs/$"
                      params={{ _splat: doc.slug }}
                      className="block p-4 rounded-xl border border-border/50 bg-card/30 hover:border-primary/50 hover:bg-card/50 transition-all group"
                    >
                      <h3 className="font-medium text-foreground group-hover:text-primary transition-colors">
                        {doc.title}
                      </h3>
                      {doc.description && (
                        <p className="mt-1 text-sm text-muted-foreground line-clamp-2">
                          {doc.description}
                        </p>
                      )}
                    </Link>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </main>

        {/* Empty space on the right to balance layout on xl screens */}
        <div className="hidden xl:block w-56 shrink-0" />
      </div>
    </section>
  );
}
