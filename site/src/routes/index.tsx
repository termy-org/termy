import { createFileRoute } from "@tanstack/react-router";
import { Hero } from "@/components/Hero";
import { SocialProof } from "@/components/SocialProof";
import { Features } from "@/components/Features";
import { Download } from "@/components/Download";
import { useGitHubRelease } from "@/hooks/useGitHubRelease";

export const Route = createFileRoute("/")({
  component: HomePage,
});

function HomePage() {
  const { release, loading, error } = useGitHubRelease();

  return (
    <>
      <Hero release={release} />
      <SocialProof />
      <Features />
      <Download release={release} loading={loading} error={error} />
    </>
  );
}
