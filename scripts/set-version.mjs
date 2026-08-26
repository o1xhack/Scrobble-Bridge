import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const repositoryRoot = resolve(import.meta.dirname, "..");
const nextVersion = process.argv[2]?.trim();
if (!nextVersion || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(nextVersion)) {
  throw new Error("Usage: pnpm version:set <semver>");
}

const jsonVersionFiles = [
  "package.json",
  "apps/desktop/package.json",
  "apps/web/package.json",
  "apps/extension/package.json",
  "apps/extension/public/manifest.json",
  "packages/ui/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

let previousVersion;
for (const relativePath of jsonVersionFiles) {
  const path = resolve(repositoryRoot, relativePath);
  const document = JSON.parse(readFileSync(path, "utf8"));
  previousVersion ??= document.version;
  document.version = nextVersion;
  writeFileSync(path, `${JSON.stringify(document, null, 2)}\n`);
}

const cargoPath = resolve(repositoryRoot, "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const nextCargo = cargo.replace(
  /(\[workspace\.package\][\s\S]*?\nversion = ")[^"]+("\n)/,
  `$1${nextVersion}$2`,
);
if (nextCargo === cargo) throw new Error("Could not update workspace package version");
writeFileSync(cargoPath, nextCargo);

for (const relativePath of ["README.md", "docs/zh-CN/README.md"]) {
  const path = resolve(repositoryRoot, relativePath);
  const contents = readFileSync(path, "utf8");
  writeFileSync(
    path,
    contents.replaceAll(
      `Scrobble Bridge_${previousVersion}_`,
      `Scrobble Bridge_${nextVersion}_`,
    ),
  );
}

const releaseNotesPath = resolve(
  repositoryRoot,
  `docs/releases/v${nextVersion}.md`,
);
if (!existsSync(releaseNotesPath)) {
  writeFileSync(
    releaseNotesPath,
    `# Scrobble Bridge ${nextVersion}\n\n## English\n\n- Describe user-visible changes here.\n\n## 简体中文\n\n- 在此填写用户可见的更新内容。\n`,
  );
}

execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], {
  cwd: repositoryRoot,
  stdio: "ignore",
});

console.log(
  `Updated Scrobble Bridge from ${previousVersion} to ${nextVersion}. Review the release notes, run the release gate, and authorize tag/release publication separately.`,
);
