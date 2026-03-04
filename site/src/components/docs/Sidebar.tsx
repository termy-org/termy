import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { getDocsByCategory, getAllDocs, sortDocCategories } from "@/lib/docs";

interface SidebarProps {
  currentSlug: string;
  search: string;
  onSearchChange: (value: string) => void;
}

export function Sidebar({ currentSlug, search, onSearchChange }: SidebarProps) {
  const docsByCategory = getDocsByCategory();
  const allDocs = getAllDocs();

  const filteredResults = useMemo(() => {
    if (!search.trim()) return null;

    const query = search.toLowerCase();
    return allDocs.filter(
      (doc) =>
        doc.title.toLowerCase().includes(query) ||
        doc.description?.toLowerCase().includes(query) ||
        doc.content.toLowerCase().includes(query),
    );
  }, [search, allDocs]);

  const categories = sortDocCategories(Object.keys(docsByCategory));

  return (
    <aside className="hidden lg:block w-64 shrink-0">
      <nav className="sticky top-24 pr-4">
        {/* Search input */}
        <div className="relative mb-4">
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
            onChange={(e) => onSearchChange(e.target.value)}
            className="w-full pl-9 pr-8 py-2 text-sm bg-secondary/50 border border-border/50 rounded-lg placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary/50 transition-colors"
          />
          {search && (
            <button
              type="button"
              onClick={() => onSearchChange("")}
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

        {/* Search results */}
        {filteredResults !== null ? (
          <div className="space-y-1">
            {filteredResults.length === 0 ? (
              <p className="text-sm text-muted-foreground px-3 py-2">
                No results found
              </p>
            ) : (
              <>
                <p className="text-xs text-muted-foreground px-3 mb-2">
                  {filteredResults.length} result
                  {filteredResults.length !== 1 ? "s" : ""}
                </p>
                {filteredResults.map((doc) => (
                  <Link
                    key={doc.slug}
                    to="/docs/$"
                    params={{ _splat: doc.slug }}
                    search={{ q: search }}
                    className={`block text-sm py-2 px-3 rounded-lg transition-colors ${
                      currentSlug === doc.slug
                        ? "bg-primary/10 text-primary font-medium"
                        : "text-muted-foreground hover:text-foreground hover:bg-secondary/50"
                    }`}
                  >
                    <span className="block">{doc.title}</span>
                    {doc.category && (
                      <span className="text-xs text-muted-foreground/70">
                        {doc.category}
                      </span>
                    )}
                  </Link>
                ))}
              </>
            )}
          </div>
        ) : (
          /* Category navigation */
          <div className="space-y-6">
            {categories.map((category) => (
              <div key={category}>
                <h4 className="text-sm font-semibold text-foreground mb-2">
                  {category}
                </h4>
                <ul className="space-y-1">
                  {docsByCategory[category].map((doc) => (
                    <li key={doc.slug}>
                      <Link
                        to="/docs/$"
                        params={{ _splat: doc.slug }}
                        className={`block text-sm py-1.5 px-3 rounded-lg transition-colors ${
                          currentSlug === doc.slug
                            ? "bg-primary/10 text-primary font-medium"
                            : "text-muted-foreground hover:text-foreground hover:bg-secondary/50"
                        }`}
                      >
                        {doc.title}
                      </Link>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </nav>
    </aside>
  );
}
