import { serveStatic } from "hono/bun";
import { Notra } from "@usenotra/sdk";
import { Hono, type Context } from "hono";

const app = new Hono();

const NOTRA_API_KEY = process.env.NOTRA_API_KEY;
const PORT = Number(process.env.PORT) || 3000;
const THEME_STORE_API_URL = process.env.THEME_STORE_API_URL || "http://127.0.0.1:8080";
const GITHUB_LATEST_RELEASE_URL =
  "https://api.github.com/repos/lassejlv/termy/releases/latest";
const GITHUB_CONTRIBUTORS_URL =
  "https://api.github.com/repos/lassejlv/termy/stats/contributors";
const GITHUB_RELEASE_CACHE_TTL_MS = Math.max(
  Number(process.env.GITHUB_RELEASE_CACHE_TTL_MS) || 3_600_000,
  1_000,
);
const GITHUB_CONTRIBUTORS_CACHE_TTL_MS = Math.max(
  Number(process.env.GITHUB_CONTRIBUTORS_CACHE_TTL_MS) || 3_600_000,
  1_000,
);

interface GitHubReleaseAsset {
  name: string;
  browser_download_url: string;
  size: number;
}

interface GitHubLatestRelease {
  tag_name: string;
  published_at: string;
  html_url: string;
  body: string;
  assets: GitHubReleaseAsset[];
}

interface GitHubReleaseCache {
  release: GitHubLatestRelease | null;
  expiresAt: number;
  etag: string | null;
}

const latestReleaseCache: GitHubReleaseCache = {
  release: null,
  expiresAt: 0,
  etag: null,
};

interface GitHubContributorWeek {
  w: number;
  a: number;
  d: number;
  c: number;
}

interface GitHubContributorAuthor {
  login: string;
  id: number;
  avatar_url: string;
  html_url: string;
}

interface GitHubContributor {
  total: number;
  weeks: GitHubContributorWeek[];
  author: GitHubContributorAuthor;
}

interface GitHubContributorsCache {
  contributors: GitHubContributor[] | null;
  expiresAt: number;
  etag: string | null;
}

const contributorsCache: GitHubContributorsCache = {
  contributors: null,
  expiresAt: 0,
  etag: null,
};

const notra = NOTRA_API_KEY ? new Notra({ bearerAuth: NOTRA_API_KEY }) : null;

const NOTRA_CACHE_TTL_MS = Math.max(
  Number(process.env.NOTRA_CACHE_TTL_MS) || 3_600_000,
  1_000,
);

interface NotraCacheEntry<T> {
  data: T;
  expiresAt: number;
}

const notraChangelogsCache: NotraCacheEntry<unknown> = {
  data: null,
  expiresAt: 0,
};

const notraPostCache = new Map<string, NotraCacheEntry<unknown>>();

function pruneExpiredPostCache(now: number): void {
  for (const [key, entry] of notraPostCache) {
    if (entry.expiresAt <= now) {
      notraPostCache.delete(key);
    }
  }
}

function getNotraCacheMaxAgeSeconds(): number {
  return Math.max(Math.floor(NOTRA_CACHE_TTL_MS / 1000), 1);
}

interface NotraConfig {
  client: Notra;
}

function respondNotConfigured(context: Context): Response {
  return context.json({ error: "Notra not configured" }, 500);
}

function getNotraConfig(context: Context): NotraConfig | Response {
  if (!notra) {
    return respondNotConfigured(context);
  }

  return {
    client: notra,
  };
}

function getReleaseCacheMaxAgeSeconds(): number {
  return Math.max(Math.floor(GITHUB_RELEASE_CACHE_TTL_MS / 1000), 1);
}

async function fetchLatestGitHubRelease(
  etag: string | null,
): Promise<Response> {
  const headers = new Headers({
    Accept: "application/vnd.github+json",
    "User-Agent": "termy-site-server",
  });

  if (etag) {
    headers.set("If-None-Match", etag);
  }

  return fetch(GITHUB_LATEST_RELEASE_URL, {
    headers,
  });
}

function hasValidReleaseCache(now: number): boolean {
  return Boolean(
    latestReleaseCache.release && latestReleaseCache.expiresAt > now,
  );
}

function setReleaseResponseHeaders(context: Context, cacheState: string): void {
  context.header(
    "Cache-Control",
    `public, max-age=${getReleaseCacheMaxAgeSeconds()}`,
  );
  context.header("X-Cache", cacheState);
}

app.get("/api/github/releases/latest", async (c) => {
  const now = Date.now();
  if (hasValidReleaseCache(now)) {
    setReleaseResponseHeaders(c, "HIT");
    return c.json(latestReleaseCache.release);
  }

  try {
    const response = await fetchLatestGitHubRelease(latestReleaseCache.etag);

    if (response.status === 304 && latestReleaseCache.release) {
      latestReleaseCache.expiresAt = now + GITHUB_RELEASE_CACHE_TTL_MS;
      setReleaseResponseHeaders(c, "REVALIDATED");
      return c.json(latestReleaseCache.release);
    }

    if (!response.ok) {
      return c.json({ error: `GitHub API returned ${response.status}` }, 502);
    }

    const release = (await response.json()) as GitHubLatestRelease;
    latestReleaseCache.release = release;
    latestReleaseCache.expiresAt = now + GITHUB_RELEASE_CACHE_TTL_MS;
    latestReleaseCache.etag = response.headers.get("ETag");

    setReleaseResponseHeaders(c, "MISS");
    return c.json(release);
  } catch {
    if (latestReleaseCache.release) {
      setReleaseResponseHeaders(c, "STALE");
      return c.json(latestReleaseCache.release);
    }

    return c.json({ error: "Failed to fetch latest release from GitHub" }, 502);
  }
});

app.get("/api/github/contributors", async (c) => {
  const now = Date.now();
  const maxAge = Math.max(Math.floor(GITHUB_CONTRIBUTORS_CACHE_TTL_MS / 1000), 1);

  if (contributorsCache.contributors && contributorsCache.expiresAt > now) {
    c.header("Cache-Control", `public, max-age=${maxAge}`);
    c.header("X-Cache", "HIT");
    return c.json(contributorsCache.contributors);
  }

  try {
    const headers = new Headers({
      Accept: "application/vnd.github+json",
      "User-Agent": "termy-site-server",
    });

    if (contributorsCache.etag) {
      headers.set("If-None-Match", contributorsCache.etag);
    }

    let response = await fetch(GITHUB_CONTRIBUTORS_URL, { headers });

    // GitHub returns 202 while computing stats — retry up to 5 times
    let retries = 0;
    while (response.status === 202 && retries < 5) {
      retries++;
      await new Promise((r) => setTimeout(r, 2000));
      response = await fetch(GITHUB_CONTRIBUTORS_URL, { headers });
    }

    if (response.status === 304 && contributorsCache.contributors) {
      contributorsCache.expiresAt = now + GITHUB_CONTRIBUTORS_CACHE_TTL_MS;
      c.header("Cache-Control", `public, max-age=${maxAge}`);
      c.header("X-Cache", "REVALIDATED");
      return c.json(contributorsCache.contributors);
    }

    if (!response.ok) {
      if (contributorsCache.contributors) {
        c.header("Cache-Control", `public, max-age=${maxAge}`);
        c.header("X-Cache", "STALE");
        return c.json(contributorsCache.contributors);
      }
      return c.json({ error: `GitHub API returned ${response.status}` }, 502);
    }

    const data = (await response.json()) as GitHubContributor[];
    const sorted = data.sort((a, b) => b.total - a.total);

    contributorsCache.contributors = sorted;
    contributorsCache.expiresAt = now + GITHUB_CONTRIBUTORS_CACHE_TTL_MS;
    contributorsCache.etag = response.headers.get("ETag");

    c.header("Cache-Control", `public, max-age=${maxAge}`);
    c.header("X-Cache", "MISS");
    return c.json(sorted);
  } catch {
    if (contributorsCache.contributors) {
      c.header("Cache-Control", `public, max-age=${maxAge}`);
      c.header("X-Cache", "STALE");
      return c.json(contributorsCache.contributors);
    }
    return c.json({ error: "Failed to fetch contributors from GitHub" }, 502);
  }
});

app.get("/api/changelogs", async (c) => {
  const now = Date.now();
  const maxAge = getNotraCacheMaxAgeSeconds();

  if (notraChangelogsCache.data && notraChangelogsCache.expiresAt > now) {
    c.header(
      "Cache-Control",
      `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
    );
    c.header("X-Cache", "HIT");
    return c.json(notraChangelogsCache.data);
  }

  const config = getNotraConfig(c);
  if (config instanceof Response) {
    return config;
  }

  try {
    const result = await config.client.content.listPosts({
      status: "published",
      contentType: "changelog",
      sort: "desc",
      limit: 100,
    });

    notraChangelogsCache.data = result;
    notraChangelogsCache.expiresAt = now + NOTRA_CACHE_TTL_MS;

    c.header(
      "Cache-Control",
      `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
    );
    c.header("X-Cache", "MISS");
    return c.json(result);
  } catch {
    if (notraChangelogsCache.data) {
      c.header(
        "Cache-Control",
        `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
      );
      c.header("X-Cache", "STALE");
      return c.json(notraChangelogsCache.data);
    }
    return c.json({ error: "Failed to fetch changelogs from Notra" }, 502);
  }
});

app.get("/api/changelogs/:id", async (c) => {
  const postId = c.req.param("id");
  const now = Date.now();
  const maxAge = getNotraCacheMaxAgeSeconds();
  const cached = notraPostCache.get(postId);

  if (cached && cached.expiresAt > now) {
    c.header(
      "Cache-Control",
      `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
    );
    c.header("X-Cache", "HIT");
    return c.json(cached.data);
  }

  const config = getNotraConfig(c);
  if (config instanceof Response) {
    return config;
  }

  try {
    const result = await config.client.content.getPost({
      postId,
    });

    pruneExpiredPostCache(now);
    notraPostCache.set(postId, { data: result, expiresAt: now + NOTRA_CACHE_TTL_MS });

    c.header(
      "Cache-Control",
      `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
    );
    c.header("X-Cache", "MISS");
    return c.json(result);
  } catch {
    if (cached) {
      c.header(
        "Cache-Control",
        `public, max-age=${maxAge}, stale-while-revalidate=${maxAge * 2}`,
      );
      c.header("X-Cache", "STALE");
      return c.json(cached.data);
    }
    return c.json({ error: "Failed to fetch changelog from Notra" }, 502);
  }
});

app.all("/theme-api/*", async (c) => {
  const incomingUrl = new URL(c.req.url);
  const proxiedPath = incomingUrl.pathname.replace(/^\/theme-api/, "") || "/";
  const targetUrl = new URL(`${proxiedPath}${incomingUrl.search}`, THEME_STORE_API_URL);

  const requestHeaders = new Headers(c.req.raw.headers);
  requestHeaders.delete("host");

  const method = c.req.method;
  const hasRequestBody = method !== "GET" && method !== "HEAD";
  const body = hasRequestBody ? await c.req.arrayBuffer() : undefined;

  const upstreamResponse = await fetch(targetUrl, {
    method,
    headers: requestHeaders,
    body,
    redirect: "manual",
  });

  return new Response(upstreamResponse.body, {
    status: upstreamResponse.status,
    headers: upstreamResponse.headers,
  });
});

app.use("/*", serveStatic({ root: "./dist" }));

// SPA fallback - serve index.html for client-side routes
app.get("/*", serveStatic({ root: "./dist", path: "index.html" }));

Bun.serve({ fetch: app.fetch, port: PORT, hostname: "0.0.0.0" });
