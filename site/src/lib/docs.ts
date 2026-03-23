import { generateSlug } from "@/lib/utils";

// Docs SDK for loading markdown content from src/content/

export interface DocMeta {
  slug: string;
  title: string;
  description?: string;
  order?: number;
  category?: string;
}

export interface Doc extends DocMeta {
  content: string;
}

interface ParsedFrontmatter {
  meta: Record<string, string>;
  content: string;
}

function toTitleFromSlug(slug: string): string {
  return slug
    .split("/")
    .pop()!
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function toDocOrder(value: string | undefined): number | undefined {
  if (!value) {
    return undefined;
  }

  return Number.parseInt(value, 10);
}

function compareDocs(a: Doc, b: Doc): number {
  if (a.order !== undefined && b.order !== undefined) {
    return a.order - b.order;
  }
  if (a.order !== undefined) return -1;
  if (b.order !== undefined) return 1;
  return a.title.localeCompare(b.title);
}

// Parse frontmatter from markdown content
function parseFrontmatter(content: string): ParsedFrontmatter {
  const frontmatterRegex = /^---\n([\s\S]*?)\n---\n/;
  const match = content.match(frontmatterRegex);

  if (!match) {
    return { meta: {}, content };
  }

  const frontmatter = match[1];
  const meta: Record<string, string> = {};

  for (const line of frontmatter.split("\n")) {
    const [key, ...valueParts] = line.split(":");
    if (key && valueParts.length > 0) {
      meta[key.trim()] = valueParts.join(":").trim();
    }
  }

  return {
    meta,
    content: content.slice(match[0].length),
  };
}

// Import all markdown files from content directory
const modules = import.meta.glob("/src/content/**/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// Process all docs
function processDoc(path: string, rawContent: string): Doc {
  // Extract slug from path: /src/content/foo/bar.md -> foo/bar or /src/content/bar.md -> bar
  const slug = path.replace("/src/content/", "").replace(".md", "");

  const { meta, content } = parseFrontmatter(rawContent);

  const title = meta.title ?? toTitleFromSlug(slug);

  return {
    slug,
    title,
    description: meta.description,
    order: toDocOrder(meta.order),
    category: meta.category,
    content,
  };
}

// Get all docs
export function getAllDocs(): Doc[] {
  return Object.entries(modules)
    .map(([path, content]) => processDoc(path, content))
    .sort(compareDocs);
}

// Get docs grouped by category
export function getDocsByCategory(): Record<string, Doc[]> {
  const docs = getAllDocs();
  const grouped: Record<string, Doc[]> = {};

  for (const doc of docs) {
    const category = doc.category ?? "General";
    grouped[category] ??= [];
    grouped[category].push(doc);
  }

  return grouped;
}

const CATEGORY_ORDER: Record<string, number> = {
  "Getting Started": 0,
  Guides: 1,
  "Help & Troubleshooting": 3,
  Architecture: 4,
  General: 99,
};

export function sortDocCategories(categories: string[]): string[] {
  return [...categories].sort((a, b) => {
    const aOrder = CATEGORY_ORDER[a] ?? 50;
    const bOrder = CATEGORY_ORDER[b] ?? 50;

    if (aOrder !== bOrder) {
      return aOrder - bOrder;
    }

    return a.localeCompare(b);
  });
}

// Get a single doc by slug
export function getDocBySlug(slug: string): Doc | undefined {
  const docs = getAllDocs();
  return docs.find((doc) => doc.slug === slug);
}

// Get all doc slugs (for static generation)
export function getAllDocSlugs(): string[] {
  return getAllDocs().map((doc) => doc.slug);
}

// Heading type for table of contents
export interface Heading {
  id: string;
  text: string;
  level: number;
}

// Extract headings from markdown content
export function extractHeadings(content: string): Heading[] {
  const headingRegex = /^(#{2,4})\s+(.+)$/gm;
  const headings: Heading[] = [];
  let match;

  while ((match = headingRegex.exec(content)) !== null) {
    const level = match[1].length;
    const text = match[2].trim();
    const id = generateSlug(text);

    headings.push({ id, text, level });
  }

  return headings;
}
