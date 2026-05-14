import { createFileRoute, Link } from '@tanstack/react-router';
import { createServerFn } from '@tanstack/react-start';
import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { ArrowUpRight } from 'lucide-react';
import { baseOptions } from '@/lib/layout.shared';
import { fetchReleases, releaseSlug, type NotraPost } from '@/lib/notra';
import { PoweredByNotra } from '@/components/powered-by-notra';

const loadReleases = createServerFn({ method: 'GET' }).handler(async () => {
  try {
    const posts = await fetchReleases();
    return { posts, error: null as string | null };
  } catch (err) {
    return {
      posts: [] as NotraPost[],
      error: err instanceof Error ? err.message : 'Failed to load releases',
    };
  }
});

export const Route = createFileRoute('/releases/')({
  component: ReleasesPage,
  loader: () => loadReleases(),
});

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

function ReleasesPage() {
  const { posts, error } = Route.useLoaderData();

  return (
    <HomeLayout {...baseOptions()}>
      <main className="flex flex-1 flex-col">
        <section className="mx-auto w-full max-w-4xl px-6 pt-24 pb-16 text-center md:pt-32">
          <span className="inline-flex items-center rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs uppercase tracking-wider text-fd-muted-foreground">
            Changelog
          </span>
          <h1 className="mt-6 font-medium text-5xl tracking-tight md:text-6xl">
            <span className="bg-gradient-to-b from-fd-foreground to-fd-muted-foreground bg-clip-text text-transparent">
              Releases
            </span>
          </h1>
          <p className="mx-auto mt-5 max-w-xl text-balance text-fd-muted-foreground md:text-lg">
            New features, fixes, and improvements shipping in Termy.
          </p>
        </section>

        <section className="mx-auto w-full max-w-3xl px-6 pb-12">
          {error && (
            <div className="rounded-lg border border-fd-border bg-fd-card p-4 text-sm text-fd-muted-foreground">
              Unable to load releases right now.
            </div>
          )}

          {!error && posts.length === 0 && (
            <div className="rounded-lg border border-fd-border bg-fd-card p-4 text-sm text-fd-muted-foreground">
              No releases yet.
            </div>
          )}

          <ul className="flex flex-col gap-3">
            {posts.map((post) => (
              <li key={post.id}>
                <Link
                  to="/releases/$slug"
                  params={{ slug: releaseSlug(post) }}
                  className="group relative flex items-center justify-between gap-6 overflow-hidden rounded-xl border border-fd-border bg-fd-card px-6 py-5 transition-all hover:border-fd-primary/40 hover:bg-fd-accent"
                >
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-0 opacity-0 transition-opacity duration-300 group-hover:opacity-100"
                    style={{
                      background:
                        'radial-gradient(600px circle at var(--mx, 50%) var(--my, 50%), color-mix(in oklch, var(--color-fd-primary) 8%, transparent), transparent 40%)',
                    }}
                  />
                  <div className="flex flex-col gap-1">
                    <time
                      dateTime={post.createdAt}
                      className="text-[11px] font-medium uppercase tracking-wider text-fd-muted-foreground"
                    >
                      {formatDate(post.createdAt)}
                    </time>
                    <h2 className="font-medium text-xl tracking-tight text-fd-foreground md:text-2xl">
                      {post.title}
                    </h2>
                  </div>
                  <ArrowUpRight
                    className="size-5 shrink-0 text-fd-muted-foreground transition-all group-hover:translate-x-0.5 group-hover:-translate-y-0.5 group-hover:text-fd-foreground"
                    strokeWidth={1.75}
                  />
                </Link>
              </li>
            ))}
          </ul>
        </section>

        <PoweredByNotra />
      </main>
    </HomeLayout>
  );
}
