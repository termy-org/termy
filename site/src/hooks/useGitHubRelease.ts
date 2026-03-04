import { useState, useEffect } from "react";

const API_URL = "/api/github/releases/latest";

export interface Asset {
  name: string;
  browser_download_url: string;
  size: number;
}

export interface Release {
  tag_name: string;
  published_at: string;
  html_url: string;
  body: string;
  assets: Asset[];
}

interface UseGitHubReleaseResult {
  release: Release | null;
  loading: boolean;
  error: string | null;
}

export function useGitHubRelease(): UseGitHubReleaseResult {
  const [release, setRelease] = useState<Release | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchRelease() {
      try {
        const response = await fetch(API_URL);

        if (!response.ok) {
          throw new Error(`GitHub API returned ${response.status}`);
        }

        const data = await response.json();
        setRelease(data);
      } catch (err) {
        setError("Could not fetch latest release. Try again in a moment.");
        console.error(err);
      } finally {
        setLoading(false);
      }
    }

    fetchRelease();
  }, []);

  return { release, loading, error };
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function classifyAssets(assets: Asset[]) {
  const isMacAsset = (asset: Asset) =>
    asset.name.toLowerCase().endsWith(".dmg");

  const isWindowsAsset = (asset: Asset) => {
    const name = asset.name.toLowerCase();
    return (
      name.endsWith(".exe") ||
      name.endsWith(".msi") ||
      name.includes("windows") ||
      name.includes("win64") ||
      name.includes("win32") ||
      name.includes("pc-windows")
    );
  };

  const isLinuxAsset = (asset: Asset) => {
    const name = asset.name.toLowerCase();
    return (
      (name.includes("linux") && name.endsWith(".tar.gz")) ||
      name.endsWith(".appimage") ||
      name.endsWith(".deb") ||
      name.endsWith(".rpm")
    );
  };

  return {
    mac: assets.filter(isMacAsset),
    windows: assets.filter(isWindowsAsset),
    linux: assets.filter(isLinuxAsset),
  };
}

export function getPreferredDownload(assets: Asset[]): Asset | undefined {
  const classified = classifyAssets(assets);
  const agent = navigator.userAgent.toLowerCase();

  if (agent.includes("win")) {
    return classified.windows[0];
  }
  if (agent.includes("mac")) {
    return (
      classified.mac.find((a) => a.name.toLowerCase().includes("arm64")) ||
      classified.mac[0]
    );
  }
  if (agent.includes("linux")) {
    return (
      classified.linux.find((a) => a.name.toLowerCase().includes("x86_64")) ||
      classified.linux[0]
    );
  }

  return (
    classified.mac.find((a) => a.name.toLowerCase().includes("arm64")) ||
    assets[0]
  );
}
