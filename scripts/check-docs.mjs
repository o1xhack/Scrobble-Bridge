import { access, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const documents = ["README.md", "docs/zh-CN/README.md"];

const requiredText = {
  "README.md": [
    "docs/zh-CN/README.md",
    "https://github.com/o1xhack/Scrobble-Bridge/releases",
    "Download",
    "Mac App Store",
  ],
  "docs/zh-CN/README.md": [
    "../../README.md",
    "https://github.com/o1xhack/Scrobble-Bridge/releases",
    "下载",
    "Mac App Store",
  ],
};

function localTargets(markdown) {
  const targets = [];
  const markdownLinks = /!?\[[^\]]*\]\(([^)]+)\)/g;
  const htmlSources = /(?:href|src)="([^"]+)"/g;

  for (const expression of [markdownLinks, htmlSources]) {
    for (const match of markdown.matchAll(expression)) {
      const raw = match[1].trim().replace(/^<|>$/g, "");
      if (
        raw.startsWith("https://") ||
        raw.startsWith("http://") ||
        raw.startsWith("mailto:") ||
        raw.startsWith("#")
      ) {
        continue;
      }
      targets.push(decodeURIComponent(raw.split("#", 1)[0].split("?", 1)[0]));
    }
  }

  return targets.filter(Boolean);
}

for (const document of documents) {
  const absoluteDocument = path.join(root, document);
  const markdown = await readFile(absoluteDocument, "utf8");

  for (const text of requiredText[document]) {
    if (!markdown.includes(text)) {
      throw new Error(`${document} is missing required text: ${text}`);
    }
  }

  for (const target of localTargets(markdown)) {
    const absoluteTarget = path.resolve(path.dirname(absoluteDocument), target);
    try {
      await access(absoluteTarget);
    } catch {
      throw new Error(`${document} contains a broken local link: ${target}`);
    }
  }
}

console.log("Documentation entrypoints and local links are valid.");
