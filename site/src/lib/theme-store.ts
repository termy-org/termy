export interface Theme {
  id: string;
  name: string;
  slug: string;
  description: string;
  latestVersion: string | null;
  fileKey: string | null;
  fileUrl: string | null;
  githubUsernameClaim: string;
  githubUserIdClaim: number | null;
  isPublic: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface ThemeVersion {
  id: string;
  themeId: string;
  version: string;
  fileKey: string;
  fileUrl: string | null;
  changelog: string;
  checksumSha256: string | null;
  createdBy: string | null;
  publishedAt: string;
  createdAt: string;
}

export interface AuthUser {
  id: string;
  githubUserId: number;
  githubLogin: string;
}

export interface ThemeWithVersionsResponse {
  theme: Theme;
  versions: ThemeVersion[];
}

interface ApiErrorBody {
  error?: string;
}

const API_BASE = "/theme-api";

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers ?? {});
  if (!(init?.body instanceof FormData)) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    credentials: "include",
    headers,
  });

  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      const body = (await response.json()) as ApiErrorBody;
      if (body.error) {
        message = body.error;
      }
    } catch {
      // keep default message
    }
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export async function fetchCurrentUser(): Promise<AuthUser | null> {
  const response = await fetch(`${API_BASE}/auth/me`, {
    credentials: "include",
  });

  if (response.status === 401) {
    return null;
  }

  if (!response.ok) {
    let message = `Could not resolve session (${response.status})`;
    try {
      const body = (await response.json()) as ApiErrorBody;
      if (body.error) {
        message = body.error;
      }
    } catch {
      // keep default message
    }
    throw new Error(message);
  }

  return (await response.json()) as AuthUser;
}

export async function logout(): Promise<void> {
  await requestJson<void>("/auth/logout", { method: "POST" });
}

export async function fetchThemes(): Promise<Theme[]> {
  return requestJson<Theme[]>("/themes", { method: "GET" });
}

export async function fetchMyThemes(): Promise<Theme[]> {
  return requestJson<Theme[]>("/themes/me", { method: "GET" });
}

export async function fetchThemeWithVersions(
  slug: string,
): Promise<ThemeWithVersionsResponse> {
  return requestJson<ThemeWithVersionsResponse>(`/themes/${slug}/versions`, {
    method: "GET",
  });
}

export async function createTheme(input: {
  name: string;
  description: string;
  isPublic: boolean;
  version: string;
  themeFile: File;
}): Promise<Theme> {
  const formData = new FormData();
  formData.append("name", input.name);
  formData.append("description", input.description);
  formData.append("isPublic", String(input.isPublic));
  formData.append("version", input.version);
  formData.append("themeFile", input.themeFile);

  return requestJson<Theme>("/themes", {
    method: "POST",
    body: formData,
  });
}

export async function updateTheme(
  slug: string,
  input: {
    name: string;
    description: string;
    isPublic: boolean;
  },
): Promise<Theme> {
  return requestJson<Theme>(`/themes/${slug}`, {
    method: "PATCH",
    body: JSON.stringify({
      name: input.name,
      description: input.description,
      isPublic: input.isPublic,
    }),
  });
}

export async function publishThemeVersion(
  slug: string,
  input: {
    version: string;
    changelog: string;
    checksumSha256: string;
    themeFile: File;
  },
): Promise<{ theme: Theme; version: ThemeVersion }> {
  const formData = new FormData();
  formData.append("version", input.version);
  formData.append("changelog", input.changelog);
  formData.append("checksumSha256", input.checksumSha256);
  formData.append("themeFile", input.themeFile);

  return requestJson<{ theme: Theme; version: ThemeVersion }>(
    `/themes/${slug}/versions`,
    {
      method: "POST",
      body: formData,
    },
  );
}

export function getThemeLoginUrl(redirectToPath: string): string {
  if (typeof window === "undefined") {
    return `${API_BASE}/auth/github/login`;
  }

  const redirectTo = `${window.location.origin}${redirectToPath}`;
  return `${API_BASE}/auth/github/login?redirect_to=${encodeURIComponent(redirectTo)}`;
}
