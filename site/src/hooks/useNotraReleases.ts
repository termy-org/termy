import { useQuery, type UseQueryResult } from "@tanstack/react-query";
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

const CHANGELOGS_STALE_TIME = 5 * 60_000;
const CHANGELOGS_GC_TIME = 30 * 60_000;

export function useNotraChangelogs(): UseQueryResult<NotraPost[], Error> {
  return useQuery({
    queryKey: ["notra-changelogs"],
    queryFn: fetchChangelogs,
    staleTime: CHANGELOGS_STALE_TIME,
    gcTime: CHANGELOGS_GC_TIME,
  });
}

export function useNotraChangelogById(
  id: string,
): UseQueryResult<NotraPost | null, Error> {
  return useQuery({
    queryKey: ["notra-changelog", id],
    queryFn: function queryById() {
      return fetchChangelogById(id);
    },
    enabled: Boolean(id),
    staleTime: CHANGELOGS_STALE_TIME,
    gcTime: CHANGELOGS_GC_TIME,
  });
}
