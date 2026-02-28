import { useState, useEffect } from "react";
import type { Release } from "./useGitHubRelease";

const OWNER = "lassejlv";
const REPO = "termy";
const API_URL = `https://api.github.com/repos/${OWNER}/${REPO}/releases`;

interface UseGitHubReleasesResult {
  releases: Release[];
  loading: boolean;
  error: string | null;
}

export function useGitHubReleases(): UseGitHubReleasesResult {
  const [releases, setReleases] = useState<Release[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchReleases() {
      try {
        const response = await fetch(API_URL, {
          headers: {
            Accept: "application/vnd.github+json",
          },
        });

        if (!response.ok) {
          throw new Error(`GitHub API returned ${response.status}`);
        }

        const data = await response.json();
        setReleases(data);
      } catch (err) {
        setError("Could not fetch releases. Try again in a moment.");
        console.error(err);
      } finally {
        setLoading(false);
      }
    }

    fetchReleases();
  }, []);

  return { releases, loading, error };
}

export function useGitHubReleaseByTag(tag: string) {
  const [release, setRelease] = useState<Release | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchRelease() {
      try {
        const response = await fetch(`${API_URL}/tags/${tag}`, {
          headers: {
            Accept: "application/vnd.github+json",
          },
        });

        if (!response.ok) {
          if (response.status === 404) {
            throw new Error(`Release ${tag} not found`);
          }
          throw new Error(`GitHub API returned ${response.status}`);
        }

        const data = await response.json();
        setRelease(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Could not fetch release");
        console.error(err);
      } finally {
        setLoading(false);
      }
    }

    if (tag) {
      fetchRelease();
    }
  }, [tag]);

  return { release, loading, error };
}
