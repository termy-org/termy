import { createFileRoute, Link } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import type { JSX } from "react";
import { Sidebar } from "@/components/docs/Sidebar";
import { Button } from "@/components/ui/button";
import { validateSearch, useDocSearchChange } from "@/hooks/useDocSearch";
import { getDocsByCategory, sortDocCategories } from "@/lib/docs";

const START_HERE_DOCS = [
  { slug: "installation", label: "Install Termy" },
  { slug: "first-steps", label: "First Steps" },
  { slug: "troubleshooting", label: "Troubleshooting" },
] as const;

export const Route = createFileRoute("/docs/")({
  component: DocsPage,
  validateSearch,
});

function DocsPage(): JSX.Element {
  const { q: search = "" } = Route.useSearch();
  const docsByCategory = getDocsByCategory();
  const categories = sortDocCategories(Object.keys(docsByCategory));
  const handleSearchChange = useDocSearchChange(Route.fullPath);

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
          <Button
            asChild
            variant="ghost"
            size="sm"
            className="lg:hidden mb-6 text-muted-foreground hover:text-foreground"
          >
            <Link to="/">
              <ChevronLeft className="w-4 h-4" />
              Back to home
            </Link>
          </Button>

          {/* Mobile search */}
          <div className="lg:hidden mb-6">
            <div className="relative">
              <svg
                className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                />
              </svg>
              <input
                type="text"
                placeholder="Search docs..."
                value={search}
                onChange={(e) => handleSearchChange(e.target.value)}
                className="w-full pl-9 pr-8 py-2 text-sm bg-secondary/50 border border-border/50 rounded-lg placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary/50 transition-colors"
              />
              {search && (
                <button
                  type="button"
                  onClick={() => handleSearchChange("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground transition-colors"
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
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              )}
            </div>
          </div>

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
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {START_HERE_DOCS.map((doc) => (
                <Link
                  key={doc.slug}
                  to="/docs/$"
                  params={{ _splat: doc.slug }}
                  className="rounded-lg border border-border/50 bg-background/50 px-4 py-3 text-sm text-foreground hover:border-primary/40 transition-colors"
                >
                  {doc.label}
                </Link>
              ))}
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
