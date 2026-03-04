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

// Parse frontmatter from markdown content
function parseFrontmatter(content: string): {
  meta: Record<string, string>;
  content: string;
} {
  const frontmatterRegex = /^---\n([\s\S]*?)\n---\n/;
  const match = content.match(frontmatterRegex);

  if (!match) {
    return { meta: {}, content };
  }

  const frontmatter = match[1];
  const meta: Record<string, string> = {};

  frontmatter.split("\n").forEach((line) => {
    const [key, ...valueParts] = line.split(":");
    if (key && valueParts.length > 0) {
      meta[key.trim()] = valueParts.join(":").trim();
    }
  });

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

  // Generate title from slug if not in frontmatter
  const title =
    meta.title ||
    slug
      .split("/")
      .pop()!
      .split("-")
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ");

  return {
    slug,
    title,
    description: meta.description,
    order: meta.order ? parseInt(meta.order, 10) : undefined,
    category: meta.category,
    content,
  };
}

// Get all docs
export function getAllDocs(): Doc[] {
  return Object.entries(modules)
    .map(([path, content]) => processDoc(path, content))
    .sort((a, b) => {
      // Sort by order if available, then by title
      if (a.order !== undefined && b.order !== undefined) {
        return a.order - b.order;
      }
      if (a.order !== undefined) return -1;
      if (b.order !== undefined) return 1;
      return a.title.localeCompare(b.title);
    });
}

// Get docs grouped by category
export function getDocsByCategory(): Record<string, Doc[]> {
  const docs = getAllDocs();
  const grouped: Record<string, Doc[]> = {};

  docs.forEach((doc) => {
    const category = doc.category || "General";
    if (!grouped[category]) {
      grouped[category] = [];
    }
    grouped[category].push(doc);
  });

  return grouped;
}

const CATEGORY_ORDER: Record<string, number> = {
  "Getting Started": 0,
  Guides: 1,
  "Help & Troubleshooting": 2,
  Architecture: 3,
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
    const id = text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/(^-|-$)/g, "");

    headings.push({ id, text, level });
  }

  return headings;
}
