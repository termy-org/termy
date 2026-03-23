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
  createTheme,
  fetchCurrentUser,
  fetchMyThemes,
  fetchThemeWithVersions,
  getThemeLoginUrl,
  logout,
  publishThemeVersion,
  updateTheme,
} from "@/lib/theme-store";

export const Route = createFileRoute("/themes/add")({
  component: ThemeAddPage,
});

export function ThemeAddPage(): JSX.Element {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [themes, setThemes] = useState<Theme[]>([]);
  const [selectedSlug, setSelectedSlug] = useState<string>("");
  const [selectedVersions, setSelectedVersions] = useState<ThemeVersion[]>([]);

  const [createName, setCreateName] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [createIsPublic, setCreateIsPublic] = useState(true);
  const [createVersion, setCreateVersion] = useState("1.0.0");
  const [createThemeFile, setCreateThemeFile] = useState<File | null>(null);
  const [createThemeJson, setCreateThemeJson] = useState("");

  const [updateName, setUpdateName] = useState("");
  const [updateDescription, setUpdateDescription] = useState("");
  const [updateIsPublic, setUpdateIsPublic] = useState(true);

  const [publishVersion, setPublishVersion] = useState("");
  const [publishChangelog, setPublishChangelog] = useState("");
  const [publishChecksum, setPublishChecksum] = useState("");
  const [publishThemeFile, setPublishThemeFile] = useState<File | null>(null);
  const [publishThemeJson, setPublishThemeJson] = useState("");

  const [isBootstrapping, setIsBootstrapping] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const selectedTheme =
    themes.find((theme) => theme.slug === selectedSlug) ?? null;
  const canEditSelectedTheme =
    Boolean(user) &&
    Boolean(selectedTheme) &&
    selectedTheme?.githubUsernameClaim.toLowerCase() ===
      user?.githubLogin.toLowerCase();

  const loginUrl = useMemo(() => getThemeLoginUrl("/add"), []);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    if (!selectedTheme) {
      return;
    }

    setUpdateName(selectedTheme.name);
    setUpdateDescription(selectedTheme.description);
    setUpdateIsPublic(selectedTheme.isPublic);
  }, [selectedTheme]);

  async function bootstrap(): Promise<void> {
    try {
      setError(null);
      setIsBootstrapping(true);
      const currentUser = await fetchCurrentUser();

      setUser(currentUser);

      if (!currentUser) {
        setThemes([]);
        setSelectedSlug("");
        setSelectedVersions([]);
        return;
      }

      const loadedThemes = await fetchMyThemes();
      setThemes(loadedThemes);

      if (loadedThemes.length > 0) {
        const firstSlug = loadedThemes[0].slug;
        setSelectedSlug(firstSlug);
        await loadThemeVersions(firstSlug);
      }
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsBootstrapping(false);
    }
  }

  async function loadThemeVersions(slug: string): Promise<void> {
    try {
      const response = await fetchThemeWithVersions(slug);
      setSelectedVersions(response.versions);
    } catch (err) {
      setSelectedVersions([]);
      setError(getErrorMessage(err));
    }
  }

  async function handleLogout(): Promise<void> {
    try {
      setError(null);
      await logout();
      setUser(null);
      setNotice("Logged out.");
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function handleCreateTheme(
    event: FormEvent<HTMLFormElement>,
  ): Promise<void> {
    event.preventDefault();
    if (!hasThemePayload(createThemeFile, createThemeJson)) {
      setError("Theme JSON file or pasted JSON is required.");
      return;
    }

    try {
      setError(null);
      setNotice(null);
      setIsSubmitting(true);

      const created = await createTheme({
        name: createName,
        description: createDescription,
        isPublic: createIsPublic,
        version: createVersion,
        themeFile: createThemeFile,
        themeJson: createThemeJson,
      });

      setThemes((prev) => [
        created,
        ...prev.filter((item) => item.id !== created.id),
      ]);
      setSelectedSlug(created.slug);
      setSelectedVersions([]);
      setCreateName("");
      setCreateDescription("");
      setCreateVersion("1.0.0");
      setCreateThemeFile(null);
      setCreateThemeJson("");
      setNotice(`Theme '${created.slug}' created.`);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  async function handleUpdateTheme(
    event: FormEvent<HTMLFormElement>,
  ): Promise<void> {
    event.preventDefault();
    if (!selectedTheme) {
      return;
    }

    try {
      setError(null);
      setNotice(null);
      setIsSubmitting(true);

      const updated = await updateTheme(selectedTheme.slug, {
        name: updateName,
        description: updateDescription,
        isPublic: updateIsPublic,
      });

      setThemes((prev) =>
        prev.map((item) => (item.id === updated.id ? updated : item)),
      );
      setNotice(`Theme '${updated.slug}' updated.`);
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
    if (!selectedTheme) {
      return;
    }
    if (!hasThemePayload(publishThemeFile, publishThemeJson)) {
      setError("Theme JSON file or pasted JSON is required.");
      return;
    }

    try {
      setError(null);
      setNotice(null);
      setIsSubmitting(true);

      await publishThemeVersion(selectedTheme.slug, {
        version: publishVersion,
        changelog: publishChangelog,
        checksumSha256: publishChecksum,
        themeFile: publishThemeFile,
        themeJson: publishThemeJson,
      });

      const loadedThemes = await fetchMyThemes();
      setThemes(loadedThemes);
      await loadThemeVersions(selectedTheme.slug);
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

  async function handleSelectTheme(slug: string): Promise<void> {
    setSelectedSlug(slug);
    setNotice(null);
    setError(null);
    await loadThemeVersions(slug);
  }

  return (
    <section className="pt-28 pb-16">
      <div className="mx-auto max-w-6xl space-y-6">
        {/* Hero */}
        <div className="text-center max-w-3xl mx-auto px-6">
          <h1
            className="text-4xl md:text-6xl font-bold tracking-tight animate-blur-in"
            style={{ animationDelay: "0ms" }}
          >
            <span className="gradient-text">releases.</span>
          </h1>
          <p
            className="mt-4 text-lg text-muted-foreground animate-blur-in"
            style={{ animationDelay: "100ms" }}
          >
            Upload theme JSON files, publish versions, and maintain your
            catalog.
          </p>
          <div
            className="mt-6 flex flex-wrap items-center justify-center gap-3 animate-blur-in"
            style={{ animationDelay: "200ms" }}
          >
            <Button asChild variant="outline">
              <Link to="/themes">Back to store</Link>
            </Button>
            {user ? (
              <>
                <div className="rounded-lg border border-border/60 bg-background/80 px-3 py-2 text-sm">
                  Signed in as{" "}
                  <span className="font-medium">@{user.githubLogin}</span>
                </div>
                <Button type="button" variant="outline" onClick={handleLogout}>
                  Log out
                </Button>
              </>
            ) : (
              <a href={loginUrl}>
                <Button type="button">Login with GitHub</Button>
              </a>
            )}
          </div>
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

        {!isBootstrapping && !user && (
          <div className="animate-blur-in" style={{ animationDelay: "300ms" }}>
            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Authentication Required</CardTitle>
                <CardDescription>
                  Sign in with GitHub to access the theme dashboard.
                </CardDescription>
              </CardHeader>
              <CardContent>
                <a href={loginUrl}>
                  <Button type="button">Login with GitHub</Button>
                </a>
              </CardContent>
            </Card>
          </div>
        )}

        {user && (
          <div
            className="grid gap-6 lg:grid-cols-[360px_minmax(0,1fr)] animate-blur-in"
            style={{ animationDelay: "300ms" }}
          >
            <Card className="border-border/60">
              <CardHeader>
                <CardTitle>Available Themes</CardTitle>
                <CardDescription>
                  {isBootstrapping
                    ? "Loading themes..."
                    : `${themes.length} themes available`}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                {themes.map((theme) => (
                  <button
                    key={theme.id}
                    type="button"
                    className={`w-full rounded-lg border px-3 py-2 text-left transition-colors ${
                      selectedSlug === theme.slug
                        ? "border-primary/60 bg-primary/10"
                        : "border-border/50 bg-background hover:border-primary/30"
                    }`}
                    onClick={() => void handleSelectTheme(theme.slug)}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <p className="font-medium">{theme.name}</p>
                      <span className="text-xs text-muted-foreground">
                        {theme.latestVersion ?? "no versions"}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      /{theme.slug}
                    </p>
                  </button>
                ))}
              </CardContent>
            </Card>

            <div className="space-y-6">
              <Card className="border-border/60">
                <CardHeader>
                  <CardTitle>Create Theme</CardTitle>
                  <CardDescription>
                    Create a new theme and upload the initial JSON file.
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <form
                    className="space-y-3"
                    onSubmit={(event) => void handleCreateTheme(event)}
                  >
                    <input
                      className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                      placeholder="Theme name"
                      value={createName}
                      onChange={(event) => setCreateName(event.target.value)}
                      disabled={!user || isSubmitting}
                    />
                    <p className="text-xs text-muted-foreground">
                      Slug is generated on the server from the theme name.
                    </p>
                    <input
                      className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                      placeholder="Initial version (e.g. 1.0.0)"
                      value={createVersion}
                      onChange={(event) => setCreateVersion(event.target.value)}
                      disabled={!user || isSubmitting}
                    />
                    <textarea
                      className="min-h-20 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                      placeholder="Description"
                      value={createDescription}
                      onChange={(event) =>
                        setCreateDescription(event.target.value)
                      }
                      disabled={!user || isSubmitting}
                    />
                    <label className="flex items-center gap-2 text-sm text-muted-foreground">
                      <input
                        type="checkbox"
                        checked={createIsPublic}
                        onChange={(event) =>
                          setCreateIsPublic(event.target.checked)
                        }
                        disabled={!user || isSubmitting}
                      />
                      Public theme
                    </label>
                    <input
                      className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm file:mr-3 file:rounded-md file:border-0 file:bg-secondary file:px-3 file:py-1.5 file:text-xs file:font-medium"
                      type="file"
                      accept=".json,application/json"
                      onChange={(event) =>
                        setCreateThemeFile(event.target.files?.[0] ?? null)
                      }
                      disabled={!user || isSubmitting}
                    />
                    <textarea
                      className="min-h-48 w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-sm"
                      placeholder='Or paste theme JSON here, for example: { "foreground": "#d1d5db" }'
                      value={createThemeJson}
                      onChange={(event) =>
                        setCreateThemeJson(event.target.value)
                      }
                      disabled={!user || isSubmitting}
                    />
                    <p className="text-xs text-muted-foreground">
                      Paste JSON or upload a file. Pasted JSON is used when both
                      are provided.
                    </p>
                    <Button type="submit" disabled={!user || isSubmitting}>
                      Create theme
                    </Button>
                  </form>
                </CardContent>
              </Card>

              <Card className="border-border/60">
                <CardHeader>
                  <CardTitle>Selected Theme</CardTitle>
                  <CardDescription>
                    {selectedTheme
                      ? `${selectedTheme.name} (${selectedTheme.slug})`
                      : "Select a theme from the list"}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  {selectedTheme && (
                    <div className="space-y-6">
                      <form
                        className="space-y-3"
                        onSubmit={(event) => void handleUpdateTheme(event)}
                      >
                        <h3 className="text-sm font-semibold">
                          Update metadata
                        </h3>
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          value={updateName}
                          onChange={(event) =>
                            setUpdateName(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <textarea
                          className="min-h-20 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          value={updateDescription}
                          onChange={(event) =>
                            setUpdateDescription(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <label className="flex items-center gap-2 text-sm text-muted-foreground">
                          <input
                            type="checkbox"
                            checked={updateIsPublic}
                            onChange={(event) =>
                              setUpdateIsPublic(event.target.checked)
                            }
                            disabled={!canEditSelectedTheme || isSubmitting}
                          />
                          Public theme
                        </label>
                        <Button
                          type="submit"
                          disabled={!canEditSelectedTheme || isSubmitting}
                        >
                          Save changes
                        </Button>
                      </form>

                      <form
                        className="space-y-3"
                        onSubmit={(event) => void handlePublishVersion(event)}
                      >
                        <h3 className="text-sm font-semibold">
                          Publish new version
                        </h3>
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          placeholder="Version (e.g. 1.2.0)"
                          value={publishVersion}
                          onChange={(event) =>
                            setPublishVersion(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          type="file"
                          accept=".json,application/json"
                          onChange={(event) =>
                            setPublishThemeFile(event.target.files?.[0] ?? null)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <textarea
                          className="min-h-48 w-full rounded-lg border border-border bg-background px-3 py-2 font-mono text-sm"
                          placeholder='Or paste the next theme JSON here, for example: { "background": "#141a24" }'
                          value={publishThemeJson}
                          onChange={(event) =>
                            setPublishThemeJson(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <p className="text-xs text-muted-foreground">
                          Paste JSON or upload a file. Pasted JSON is used when
                          both are provided.
                        </p>
                        <textarea
                          className="min-h-20 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          placeholder="Changelog"
                          value={publishChangelog}
                          onChange={(event) =>
                            setPublishChangelog(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <input
                          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          placeholder="Checksum SHA256 (optional)"
                          value={publishChecksum}
                          onChange={(event) =>
                            setPublishChecksum(event.target.value)
                          }
                          disabled={!canEditSelectedTheme || isSubmitting}
                        />
                        <Button
                          type="submit"
                          disabled={!canEditSelectedTheme || isSubmitting}
                        >
                          Publish version
                        </Button>
                      </form>

                      <div>
                        <h3 className="mb-2 text-sm font-semibold">
                          Version history
                        </h3>
                        <div className="space-y-2">
                          {selectedVersions.map((version) => (
                            <div
                              key={version.id}
                              className="rounded-lg border border-border/60 px-3 py-2"
                            >
                              <div className="flex items-center justify-between gap-3">
                                <span className="font-medium">
                                  {version.version}
                                </span>
                                <span className="text-xs text-muted-foreground">
                                  {new Date(
                                    version.publishedAt,
                                  ).toLocaleString()}
                                </span>
                              </div>
                              <p className="mt-1 text-xs text-muted-foreground">
                                {version.fileKey}
                              </p>
                              {version.changelog && (
                                <p className="mt-2 text-sm text-muted-foreground">
                                  {version.changelog}
                                </p>
                              )}
                            </div>
                          ))}
                          {selectedVersions.length === 0 && (
                            <p className="text-sm text-muted-foreground">
                              No versions published yet.
                            </p>
                          )}
                        </div>
                      </div>
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>
          </div>
        )}
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
