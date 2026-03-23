import type { JSX } from "react";
import { BarChart, Bar, ResponsiveContainer, Tooltip } from "recharts";
import type { ContributorWeek } from "@/hooks/useContributors";

interface ActivityChartProps {
  weeks: ContributorWeek[];
}

interface WeekData {
  commits: number;
  label: string;
}

function formatWeekLabel(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

export function ActivityChart({ weeks }: ActivityChartProps): JSX.Element {
  const recentWeeks = weeks.slice(-12);

  const data: WeekData[] = recentWeeks.map((week) => ({
    commits: week.c,
    label: formatWeekLabel(week.w),
  }));

  return (
    <div className="w-full h-12">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} barCategoryGap={2}>
          <Tooltip
            cursor={false}
            contentStyle={{
              backgroundColor: "hsl(var(--card))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "8px",
              fontSize: "12px",
              padding: "4px 8px",
            }}
            labelStyle={{ color: "hsl(var(--muted-foreground))" }}
            itemStyle={{ color: "hsl(var(--foreground))" }}
            formatter={(value: unknown) => [`${value} commits`, ""]}
            labelFormatter={(label: unknown) => String(label)}
          />
          <Bar
            dataKey="commits"
            fill="hsl(var(--primary))"
            fillOpacity={0.5}
            radius={[2, 2, 0, 0]}
            name="commits"
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
