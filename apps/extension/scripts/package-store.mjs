import { execFileSync } from "node:child_process";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";

const extensionRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(extensionRoot, "../..");
const stagingRoot = mkdtempSync(join(tmpdir(), "scrobble-bridge-extension-"));
const stagedExtension = join(stagingRoot, "extension");

try {
  cpSync(resolve(extensionRoot, "dist"), stagedExtension, { recursive: true });

  const manifestPath = join(stagedExtension, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  delete manifest.key;
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  const output = resolve(
    repositoryRoot,
    "target",
    `scrobble-bridge-extension-${manifest.version}.zip`,
  );
  mkdirSync(resolve(repositoryRoot, "target"), { recursive: true });
  rmSync(output, { force: true });
  execFileSync("zip", ["-q", "-r", output, "."], {
    cwd: stagedExtension,
    stdio: "inherit",
  });
  console.log(
    `Created Chrome Web Store package: ${relative(repositoryRoot, output)}`,
  );
} finally {
  rmSync(stagingRoot, { recursive: true, force: true });
}
