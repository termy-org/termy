import { useQuery } from "@tanstack/react-query";
import type {
  ListPostsPost,
  ListPostsResponse,
  GetPostResponse,
} from "@usenotra/sdk/models/operations";

export type NotraPost = ListPostsPost;

async function fetchChangelogs(): Promise<NotraPost[]> {
  const res = await fetch("/api/changelogs");

  if (!res.ok) {
    throw new Error(`Failed to load changelogs (${res.status})`);
  }

  const data: ListPostsResponse = await res.json();
  return data.posts;
}

async function fetchChangelogById(id: string): Promise<NotraPost | null> {
  const res = await fetch(`/api/changelogs/${encodeURIComponent(id)}`);

  if (!res.ok) {
    throw new Error(`Failed to load changelog (${res.status})`);
  }

  const data: GetPostResponse = await res.json();
  return data.post ?? null;
}

export function useNotraChangelogs() {
  return useQuery({
    queryKey: ["notra-changelogs"],
    queryFn: fetchChangelogs,
  });
}

export function useNotraChangelogById(id: string) {
  return useQuery({
    queryKey: ["notra-changelog", id],
    queryFn: () => fetchChangelogById(id),
    enabled: !!id,
  });
}
