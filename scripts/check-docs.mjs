import { access, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const documents = ["README.md", "docs/zh-CN/README.md"];

const requiredText = {
  "README.md": [
    "docs/zh-CN/README.md",
    "https://github.com/o1xhack/Scrobble-Bridge/releases/download/v1.0.0/",
    "Download",
    "runtime-tested only on an Apple silicon Mac",
    "Windows and Docker/NAS builds are **Experimental**",
    "Windows installer is unsigned",
  ],
  "docs/zh-CN/README.md": [
    "../../README.md",
    "https://github.com/o1xhack/Scrobble-Bridge/releases/download/v1.0.0/",
    "下载",
    "只在 Apple Silicon Mac 上做过运行测试",
    "Windows 和 Docker/NAS 版本均为 **Experimental（实验性版本）**",
    "Windows 安装程序未签名",
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
