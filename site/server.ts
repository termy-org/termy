import { serveStatic } from "hono/bun";
import { Notra } from "@usenotra/sdk";
import { Hono, type Context } from "hono";

const app = new Hono();

const NOTRA_ORG_ID = process.env.NOTRA_ORG_ID;
const NOTRA_API_KEY = process.env.NOTRA_API_KEY;
const PORT = Number(process.env.PORT) || 3000;
const GITHUB_LATEST_RELEASE_URL =
  "https://api.github.com/repos/lassejlv/termy/releases/latest";
const GITHUB_RELEASE_CACHE_TTL_MS = Math.max(
  Number(process.env.GITHUB_RELEASE_CACHE_TTL_MS) || 3_600_000,
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

const notra = NOTRA_API_KEY ? new Notra({ bearerAuth: NOTRA_API_KEY }) : null;

interface NotraConfig {
  client: Notra;
  organizationId: string;
}

function respondNotConfigured(context: Context): Response {
  return context.json({ error: "Notra not configured" }, 500);
}

function getNotraConfig(context: Context): NotraConfig | Response {
  if (!notra || !NOTRA_ORG_ID) {
    return respondNotConfigured(context);
  }

  return {
    client: notra,
    organizationId: NOTRA_ORG_ID,
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

app.get("/api/changelogs", async (c) => {
  const config = getNotraConfig(c);
  if (config instanceof Response) {
    return config;
  }

  try {
    const result = await config.client.content.listPosts({
      organizationId: config.organizationId,
      status: "published",
      contentType: "changelog",
      sort: "desc",
      limit: 100,
    });

    c.header("Cache-Control", "public, max-age=300");
    return c.json(result);
  } catch {
    return c.json({ error: "Failed to fetch changelogs from Notra" }, 502);
  }
});

app.get("/api/changelogs/:id", async (c) => {
  const config = getNotraConfig(c);
  if (config instanceof Response) {
    return config;
  }

  try {
    const result = await config.client.content.getPost({
      organizationId: config.organizationId,
      postId: c.req.param("id"),
    });

    c.header("Cache-Control", "public, max-age=300");
    return c.json(result);
  } catch {
    return c.json({ error: "Failed to fetch changelog from Notra" }, 502);
  }
});

app.use("/*", serveStatic({ root: "./dist" }));

// SPA fallback - serve index.html for client-side routes
app.get("/*", serveStatic({ root: "./dist", path: "index.html" }));

Bun.serve({ fetch: app.fetch, port: PORT, hostname: "0.0.0.0" });
