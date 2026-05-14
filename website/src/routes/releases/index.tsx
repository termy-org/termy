import { createFileRoute, Link } from '@tanstack/react-router';
import { createServerFn } from '@tanstack/react-start';
import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { baseOptions } from '@/lib/layout.shared';
import { fetchReleases, releaseSlug, type NotraPost } from '@/lib/notra';
import { Markdown } from '@/components/markdown';

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
        <section className="mx-auto w-full max-w-3xl px-6 pt-20 pb-12">
          <h1 className="font-medium text-4xl tracking-tight md:text-5xl">
            Releases
          </h1>
          <p className="mt-4 text-fd-muted-foreground">
            What's new in Termy.
          </p>
        </section>

        <section className="mx-auto w-full max-w-3xl px-6 pb-24">
          {error && (
            <div className="rounded-md border border-fd-border bg-fd-card p-4 text-sm text-fd-muted-foreground">
              Unable to load releases right now.
            </div>
          )}

          {!error && posts.length === 0 && (
            <div className="rounded-md border border-fd-border bg-fd-card p-4 text-sm text-fd-muted-foreground">
              No releases yet.
            </div>
          )}

          <ul className="flex flex-col gap-12">
            {posts.map((post) => (
              <li
                key={post.id}
                className="flex flex-col gap-4 border-b border-fd-border pb-12 last:border-b-0"
              >
                <div className="flex flex-col gap-1">
                  <time
                    dateTime={post.createdAt}
                    className="text-xs uppercase tracking-wide text-fd-muted-foreground"
                  >
                    {formatDate(post.createdAt)}
                  </time>
                  <h2 className="font-medium text-2xl tracking-tight">
                    <Link
                      to="/releases/$slug"
                      params={{ slug: releaseSlug(post) }}
                      className="hover:underline"
                    >
                      {post.title}
                    </Link>
                  </h2>
                </div>
                <div className="prose prose-sm max-w-none text-fd-foreground">
                  <Markdown text={post.markdown || post.content} />
                </div>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </HomeLayout>
  );
}
