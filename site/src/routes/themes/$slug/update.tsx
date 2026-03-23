import { Link, createFileRoute } from "@tanstack/react-router";
import type { FormEvent, JSX } from "react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  type AuthUser,
  type Theme,
  type ThemeVersion,
  fetchCurrentUser,
  fetchThemeWithVersions,
  getThemeLoginUrl,
  publishThemeVersion,
  updateTheme,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/$slug/update")({
  component: ThemeUpdatePage,
});

function ThemeUpdatePage(): JSX.Element {
  const { slug } = Route.useParams();

  const [user, setUser] = useState<AuthUser | null>(null);
  const [theme, setTheme] = useState<Theme | null>(null);
  const [versions, setVersions] = useState<ThemeVersion[]>([]);

  const [updateName, setUpdateName] = useState("");
  const [updateDescription, setUpdateDescription] = useState("");
  const [updateIsPublic, setUpdateIsPublic] = useState(true);

  const [publishVersion, setPublishVersion] = useState("");
  const [publishChangelog, setPublishChangelog] = useState("");
  const [publishChecksum, setPublishChecksum] = useState("");
  const [publishThemeFile, setPublishThemeFile] = useState<File | null>(null);
  const [publishThemeJson, setPublishThemeJson] = useState("");

  const [loading, setLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loginUrl = useMemo(
    () => getThemeLoginUrl(`/themes/${slug}/update`),
    [slug],
  );

  const isOwner =
    Boolean(user) &&
    Boolean(theme) &&
    (theme?.githubUserIdClaim != null
      ? theme.githubUserIdClaim === user?.githubUserId
      : theme?.githubUsernameClaim.toLowerCase() ===
        user?.githubLogin.toLowerCase());

  useEffect(() => {
    void bootstrap();
  }, [slug]);

  async function bootstrap(): Promise<void> {
    try {
      setLoading(true);
      setError(null);
      const [currentUser, response] = await Promise.all([
        fetchCurrentUser().catch(() => null),
        fetchThemeWithVersions(slug),
      ]);
      setUser(currentUser);
      setTheme(response.theme);
      setVersions(response.versions);
      setUpdateName(response.theme.name);
      setUpdateDescription(response.theme.description);
      setUpdateIsPublic(response.theme.isPublic);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleUpdateTheme(
    event: FormEvent<HTMLFormElement>,
  ): Promise<void> {
    event.preventDefault();
    if (!theme || !isOwner) {
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      setNotice(null);
      const updated = await updateTheme(theme.slug, {
        name: updateName,
        description: updateDescription,
        isPublic: updateIsPublic,
      });
      setTheme(updated);
      setNotice("Theme metadata updated.");
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  async function handlePublishVersion(
    event: FormEvent<HTMLFormElement>,
  ): Promise<void> {
    event.preventDefault();
    if (!theme || !isOwner) {
      return;
    }
    if (!hasThemePayload(publishThemeFile, publishThemeJson)) {
      setError("Theme JSON file or pasted JSON is required.");
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      setNotice(null);
      const response = await publishThemeVersion(theme.slug, {
        version: publishVersion,
        changelog: publishChangelog,
        checksumSha256: publishChecksum,
        themeFile: publishThemeFile,
        themeJson: publishThemeJson,
      });
      setTheme(response.theme);
      setVersions((previous) => [response.version, ...previous]);
      setPublishVersion("");
      setPublishChangelog("");
      setPublishChecksum("");
      setPublishThemeFile(null);
      setPublishThemeJson("");
      setNotice("Version published.");
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  if (loading) {
    return (
      <section className="pt-28 pb-16">
        <div className="mx-auto max-w-4xl rounded-xl border border-border/60 bg-card/50 px-4 py-6 text-center text-sm text-muted-foreground">
          Loading theme...
        </div>
      </section>
    );
  }

  if (!theme) {
    return (
      <section className="pt-28 pb-16">
        <div className="mx-auto max-w-4xl space-y-4">
          <Button asChild variant="outline">
            <Link to="/themes">Back to store</Link>
          </Button>
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            Theme not found.
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-4xl space-y-6">
        <div className="flex flex-wrap items-center gap-3">
          <Button asChild variant="outline">
            <Link to="/themes/$slug" params={{ slug: theme.slug }}>
              Back to theme
            </Link>
          </Button>
          <Button asChild variant="outline">
            <Link to="/themes">Back to store</Link>
          </Button>
        </div>

        {/* Hero */}
        <div className="text-center max-w-3xl mx-auto px-6">
          <h1
            className="text-4xl md:text-6xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            <span className="gradient-text">{theme.name}</span>
          </h1>
          <p
            className="mt-4 text-muted-foreground animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            Owner: @{theme.githubUsernameClaim}
          </p>
        </div>

        {error && (
          <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}

        {notice && (
          <div className="rounded-xl border border-primary/40 bg-primary/10 px-4 py-3 text-sm text-foreground">
            {notice}
          </div>
        )}

        {!user && (
          <div className="animate-blur-in" style={{ animationDelay: "200ms" }}>
            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Authentication Required</CardTitle>
                <CardDescription>
                  You must sign in with GitHub to update this theme.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <a href={loginUrl}>
                  <Button>Login with GitHub</Button>
                </a>
              </CardContent>
            </Card>
          </div>
        )}

        {user && !isOwner && (
          <Card className="border-destructive/40">
            <CardHeader>
              <CardTitle>Permission Denied</CardTitle>
              <CardDescription>
                This theme is owned by @{theme.githubUsernameClaim}. You are
                signed in as @{user.githubLogin}.
              </CardDescription>
            </CardHeader>
          </Card>
        )}

        {user && isOwner && (
          <div
            className="space-y-6 animate-blur-in"
            style={{ animationDelay: "200ms" }}
          >
            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Update metadata</CardTitle>
                <CardDescription>
                  Change name, description, and visibility.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <form
                  className="space-y-3"
                  onSubmit={(event) => void handleUpdateTheme(event)}
                >
                  <input
                    className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    value={updateName}
                    onChange={(event) => setUpdateName(event.target.value)}
                    disabled={isSubmitting}
                  />
                  <textarea
                    className="min-h-20 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    value={updateDescription}
                    onChange={(event) =>
                      setUpdateDescription(event.target.value)
                    }
                    disabled={isSubmitting}
                  />
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <input
                      type="checkbox"
                      checked={updateIsPublic}
                      onChange={(event) =>
                        setUpdateIsPublic(event.target.checked)
                      }
                      disabled={isSubmitting}
                    />
                    Public theme
                  </label>
                  <Button type="submit" disabled={isSubmitting}>
                    Save changes
                  </Button>
                </form>
              </CardContent>
            </Card>

            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Publish new version</CardTitle>
                <CardDescription>
                  Version must be valid semver and greater than current latest.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <form
                  className="space-y-3"
                  onSubmit={(event) => void handlePublishVersion(event)}
                >
                  <input
                    className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    placeholder="Version (e.g. 1.2.0)"
                    value={publishVersion}
                    onChange={(event) => setPublishVersion(event.target.value)}
                    disabled={isSubmitting}
                  />
                  <input
                    className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    type="file"
                    accept=".json,application/json"
                    onChange={(event) =>
                      setPublishThemeFile(event.target.files?.[0] ?? null)
                    }
                    disabled={isSubmitting}
                  />
                  <textarea
                    className="min-h-48 w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-sm"
                    placeholder='Or paste the next theme JSON here, for example: { "background": "#141a24" }'
                    value={publishThemeJson}
                    onChange={(event) =>
                      setPublishThemeJson(event.target.value)
                    }
                    disabled={isSubmitting}
                  />
                  <p className="text-xs text-muted-foreground">
                    Paste JSON or upload a file. Pasted JSON is used when both
                    are provided.
                  </p>
                  <textarea
                    className="min-h-20 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    placeholder="Changelog"
                    value={publishChangelog}
                    onChange={(event) =>
                      setPublishChangelog(event.target.value)
                    }
                    disabled={isSubmitting}
                  />
                  <input
                    className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                    placeholder="Checksum SHA256 (optional)"
                    value={publishChecksum}
                    onChange={(event) => setPublishChecksum(event.target.value)}
                    disabled={isSubmitting}
                  />
                  <Button type="submit" disabled={isSubmitting}>
                    Publish version
                  </Button>
                </form>
              </CardContent>
            </Card>
          </div>
        )}

        <div className="animate-blur-in" style={{ animationDelay: "300ms" }}>
          <Card className="border-border/60">
            <CardHeader>
              <CardTitle>Version history</CardTitle>
              <CardDescription>
                {versions.length} published versions
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              {versions.map((version) => (
                <div
                  key={version.id}
                  className="rounded-lg border border-border/60 border-l-2 border-l-green-500/60 px-4 py-3"
                >
                  <div className="flex items-center justify-between gap-3">
                    <span className="font-medium">{version.version}</span>
                    <span className="text-xs text-muted-foreground">
                      {new Date(version.publishedAt).toLocaleString()}
                    </span>
                  </div>
                  {version.changelog && (
                    <p className="mt-2 text-sm text-muted-foreground">
                      {version.changelog}
                    </p>
                  )}
                </div>
              ))}
            </CardContent>
          </Card>
        </div>
      </div>
    </section>
  );
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return "Unexpected error";
}

function hasThemePayload(themeFile: File | null, themeJson: string): boolean {
  return themeFile !== null || themeJson.trim().length > 0;
}
