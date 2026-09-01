import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function createUpdaterManifest({
  version,
  tag = `v${version}`,
  repository = "o1xhack/Scrobble-Bridge",
  notes,
  pubDate = new Date().toISOString(),
  platforms,
}) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`Invalid updater version: ${version}`);
  }
  if (!platforms || Object.keys(platforms).length === 0) {
    throw new Error("At least one updater platform artifact is required");
  }

  const releaseBase = `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}`;
  const manifestPlatforms = {};
  for (const [platform, artifactPath] of Object.entries(platforms)) {
    const signaturePath = `${artifactPath}.sig`;
    if (!existsSync(artifactPath)) {
      throw new Error(`Updater artifact is missing: ${artifactPath}`);
    }
    if (!existsSync(signaturePath)) {
      throw new Error(`Updater signature is missing: ${signaturePath}`);
    }
    const signature = readFileSync(signaturePath, "utf8").trim();
    if (!signature)
      throw new Error(`Updater signature is empty: ${signaturePath}`);
    const releaseAssetName = basename(artifactPath).replaceAll(" ", ".");
    manifestPlatforms[platform] = {
      signature,
      url: `${releaseBase}/${encodeURIComponent(releaseAssetName)}`,
    };
  }

  return {
    version,
    notes: notes.trim(),
    pub_date: pubDate,
    platforms: manifestPlatforms,
  };
}

function parseArguments(args) {
  const options = { platforms: {} };
  for (let index = 0; index < args.length; index += 1) {
    const name = args[index];
    const value = args[index + 1];
    if (!value) throw new Error(`Missing value for ${name}`);
    index += 1;
    if (name === "--platform") {
      const separator = value.indexOf("=");
      if (separator < 1) {
        throw new Error("--platform must use <target>=<artifact-path>");
      }
      options.platforms[value.slice(0, separator)] = resolve(
        value.slice(separator + 1),
      );
    } else if (name === "--version") options.version = value;
    else if (name === "--tag") options.tag = value;
    else if (name === "--repository") options.repository = value;
    else if (name === "--notes") options.notesPath = resolve(value);
    else if (name === "--output") options.output = resolve(value);
    else if (name === "--pub-date") options.pubDate = value;
    else throw new Error(`Unknown argument: ${name}`);
  }
  return options;
}

function assertConfiguredVersion(repositoryRoot, version) {
  const configured = [
    JSON.parse(readFileSync(resolve(repositoryRoot, "package.json"), "utf8"))
      .version,
    JSON.parse(
      readFileSync(
        resolve(repositoryRoot, "apps/desktop/src-tauri/tauri.conf.json"),
        "utf8",
      ),
    ).version,
    readFileSync(resolve(repositoryRoot, "Cargo.toml"), "utf8").match(
      /\[workspace\.package\][\s\S]*?\nversion = "([^"]+)"/,
    )?.[1],
  ];
  if (configured.some((candidate) => candidate !== version)) {
    throw new Error(
      `Version mismatch: requested ${version}, configured ${configured.join(", ")}`,
    );
  }
}

const isCommand =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
if (isCommand) {
  const repositoryRoot = resolve(import.meta.dirname, "../../..");
  const options = parseArguments(process.argv.slice(2));
  if (!options.version || !options.output) {
    throw new Error(
      "Usage: generate-updater-manifest --version <semver> --output <latest.json> --platform <target>=<artifact> [--platform ...]",
    );
  }
  assertConfiguredVersion(repositoryRoot, options.version);
  const notesPath =
    options.notesPath ??
    resolve(repositoryRoot, `docs/releases/v${options.version}.md`);
  const manifest = createUpdaterManifest({
    version: options.version,
    tag: options.tag,
    repository: options.repository,
    notes: readFileSync(notesPath, "utf8"),
    pubDate: options.pubDate,
    platforms: options.platforms,
  });
  writeFileSync(options.output, `${JSON.stringify(manifest, null, 2)}\n`);
}
