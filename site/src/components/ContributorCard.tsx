import type { JSX } from "react";
import type { Contributor } from "@/hooks/useContributors";
import { ActivityChart } from "@/components/ActivityChart";

interface ContributorCardProps {
  contributor: Contributor;
  rank: number;
  index: number;
}

function getRankBadge(rank: number): string | null {
  if (rank === 1) return "🥇";
  if (rank === 2) return "🥈";
  if (rank === 3) return "🥉";
  return null;
}

function getTotalAdditions(contributor: Contributor): number {
  return contributor.weeks.reduce((sum, week) => sum + week.a, 0);
}

function getTotalDeletions(contributor: Contributor): number {
  return contributor.weeks.reduce((sum, week) => sum + week.d, 0);
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

export function ContributorCard({ contributor, rank, index }: ContributorCardProps): JSX.Element {
  const badge = getRankBadge(rank);
  const additions = getTotalAdditions(contributor);
  const deletions = getTotalDeletions(contributor);

  return (
    <div
      className="animate-blur-in bg-card/30 border border-border/40 rounded-xl p-5 transition-all hover:border-primary/20 hover:bg-card/60"
      style={{ animationDelay: `${Math.min(index * 80, 600)}ms` }}
    >
      <div className="flex items-center gap-3 mb-4">
        <img
          src={contributor.author.avatar_url}
          alt={contributor.author.login}
          width={48}
          height={48}
          className="rounded-full"
          loading="lazy"
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <a
              href={contributor.author.html_url}
              target="_blank"
              rel="noreferrer"
              className="font-semibold text-foreground hover:text-primary transition-colors truncate"
            >
              {contributor.author.login}
            </a>
            {badge && <span className="text-lg">{badge}</span>}
          </div>
          <div className="flex items-center gap-3 text-sm text-muted-foreground mt-0.5">
            <span>{contributor.total} commits</span>
            <span className="text-green-500">+{formatNumber(additions)}</span>
            <span className="text-red-500">-{formatNumber(deletions)}</span>
          </div>
        </div>
      </div>

      <ActivityChart weeks={contributor.weeks} />
    </div>
  );
}
