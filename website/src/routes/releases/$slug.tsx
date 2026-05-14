import { createFileRoute, Link, notFound } from '@tanstack/react-router';
import { createServerFn } from '@tanstack/react-start';
import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { ArrowLeft } from 'lucide-react';
import { baseOptions } from '@/lib/layout.shared';
import { fetchReleaseBySlug } from '@/lib/notra';
import { Markdown } from '@/components/markdown';
import { PoweredByNotra } from '@/components/powered-by-notra';

const loadRelease = createServerFn({ method: 'GET' })
  .inputValidator((slug: string) => slug)
  .handler(async ({ data: slug }) => {
    const post = await fetchReleaseBySlug(slug);
    if (!post) return { post: null, notFound: true as const };
    return { post, notFound: false as const };
  });

export const Route = createFileRoute('/releases/$slug')({
  component: ReleaseDetail,
  loader: async ({ params }) => {
    const result = await loadRelease({ data: params.slug });
    if (result.notFound) throw notFound();
    return result;
  },
});

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

function ReleaseDetail() {
  const { post } = Route.useLoaderData();
  if (!post) return null;

  return (
    <HomeLayout {...baseOptions()}>
      <main className="flex flex-1 flex-col">
        <article className="mx-auto w-full max-w-3xl px-6 pt-20 pb-12">
          <Link
            to="/releases"
            className="inline-flex items-center gap-1.5 text-sm text-fd-muted-foreground hover:text-fd-foreground"
          >
            <ArrowLeft className="size-4" strokeWidth={1.75} />
            Releases
          </Link>

          <header className="mt-8 flex flex-col gap-2 border-b border-fd-border pb-8">
            <time
              dateTime={post.createdAt}
              className="text-xs uppercase tracking-wide text-fd-muted-foreground"
            >
              {formatDate(post.createdAt)}
            </time>
            <h1 className="font-medium text-4xl tracking-tight md:text-5xl">
              {post.title}
            </h1>
          </header>

          <div className="prose prose-sm mt-8 max-w-none text-fd-foreground">
            <Markdown text={post.markdown || post.content} />
          </div>
        </article>

        <PoweredByNotra />
      </main>
    </HomeLayout>
  );
}
