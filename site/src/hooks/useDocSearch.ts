import { useNavigate } from "@tanstack/react-router";
import { useCallback } from "react";

export type SearchParams = { q?: string };
type DocsRoutePath = "/docs/" | "/docs/$";

export function validateSearch(search: Record<string, unknown>): SearchParams {
  return {
    q: typeof search.q === "string" ? search.q : undefined,
  };
}

export function useDocSearchChange(from: DocsRoutePath): (value: string) => void {
  const navigate = useNavigate({ from });

  return useCallback(
    function handleSearchChange(value: string): void {
      navigate({
        search: value ? { q: value } : {},
        replace: true,
      });
    },
    [navigate],
  );
}
