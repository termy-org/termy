import { createFileRoute } from "@tanstack/react-router";
import { ThemeAddPage } from "@/routes/themes/add";

export const Route = createFileRoute("/add")({
  component: ThemeAddPage,
});
