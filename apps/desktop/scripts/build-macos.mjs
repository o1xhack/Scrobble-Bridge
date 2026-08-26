import { execFileSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { resolve } from "node:path";
import { createSigningEnvironment } from "./signing-environment.mjs";

if (process.platform !== "darwin")
  throw new Error("The macOS bundle script must run on macOS");

function detectDeveloperIdIdentity() {
  try {
    const identities = execFileSync(
      "security",
      ["find-identity", "-v", "-p", "codesigning"],
      { encoding: "utf8" },
    );
    return identities.match(/"(Developer ID Application: [^"]+)"/)?.[1];
  } catch {
    return undefined;
  }
}

const appRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(appRoot, "../..");
const appVersion = JSON.parse(
  readFileSync(resolve(appRoot, "src-tauri/tauri.conf.json"), "utf8"),
).version;
const requestedTarget = process.env.SCROBBLE_BUILD_TARGET?.trim();
const signingIdentity =
  process.env.APPLE_SIGNING_IDENTITY?.trim() || detectDeveloperIdIdentity();
const tauriArguments = [
  "run",
  "tauri",
  "build",
  "--bundles",
  "app",
  "--config",
  "src-tauri/tauri.release.conf.json",
];
if (requestedTarget) tauriArguments.push("--target", requestedTarget);
const buildEnvironment = createSigningEnvironment(process.env, signingIdentity);
const localUpdaterKey = resolve(
  process.env.HOME ?? "",
  ".codex-secrets/scrobble-bridge/updater.key",
);
if (
  !buildEnvironment.TAURI_SIGNING_PRIVATE_KEY &&
  existsSync(localUpdaterKey)
) {
  buildEnvironment.TAURI_SIGNING_PRIVATE_KEY = localUpdaterKey;
  try {
    buildEnvironment.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = execFileSync(
      "security",
      [
        "find-generic-password",
        "-s",
        "com.scrobblebridge.updater-signing",
        "-a",
        "updater-key-password",
        "-w",
      ],
      { encoding: "utf8" },
    ).trim();
  } catch {
    throw new Error(
      "The local updater signing key exists, but its Keychain password is unavailable.",
    );
  }
}
execFileSync("pnpm", tauriArguments, {
  cwd: appRoot,
  env: buildEnvironment,
  stdio: "inherit",
});
const bundleRoot = resolve(
  repositoryRoot,
  requestedTarget
    ? `target/${requestedTarget}/release/bundle`
    : "target/release/bundle",
);
const appPath = resolve(bundleRoot, "macos/Scrobble Bridge.app");
const updaterSource = `${appPath}.tar.gz`;
const updaterSignatureSource = `${updaterSource}.sig`;
if (!existsSync(updaterSource) || !existsSync(updaterSignatureSource)) {
  throw new Error(
    "The signed macOS updater archive and signature were not generated.",
  );
}
if (signingIdentity) {
  execFileSync("codesign", ["--verify", "--deep", "--strict", appPath], {
    stdio: "inherit",
  });
} else {
  console.warn(
    "No Developer ID Application identity was found; using an ad-hoc signature. Keychain access may require approval after each rebuild.",
  );
  execFileSync("codesign", ["--force", "--deep", "--sign", "-", appPath], {
    stdio: "inherit",
  });
}
const staging = resolve(bundleRoot, "dmg-staging");
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });
cpSync(appPath, resolve(staging, "Scrobble Bridge.app"), {
  recursive: true,
  force: true,
});
try {
  symlinkSync("/Applications", resolve(staging, "Applications"));
} catch (error) {
  if (error?.code !== "EEXIST") throw error;
}
const architecture = requestedTarget?.startsWith("x86_64-")
  ? "x86_64"
  : requestedTarget?.startsWith("aarch64-") || process.arch === "arm64"
    ? "aarch64"
    : "x86_64";
const updaterDirectory = resolve(bundleRoot, "updater");
const updaterArtifact = resolve(
  updaterDirectory,
  `Scrobble Bridge_${appVersion}_${architecture}.app.tar.gz`,
);
mkdirSync(updaterDirectory, { recursive: true });
cpSync(updaterSource, updaterArtifact, { force: true });
cpSync(updaterSignatureSource, `${updaterArtifact}.sig`, { force: true });
const output = resolve(
  bundleRoot,
  `dmg/Scrobble Bridge_${appVersion}_${architecture}.dmg`,
);
mkdirSync(resolve(bundleRoot, "dmg"), { recursive: true });
execFileSync(
  "hdiutil",
  [
    "create",
    "-volname",
    "Scrobble Bridge",
    "-srcfolder",
    staging,
    "-ov",
    "-format",
    "UDZO",
    output,
  ],
  {
    stdio: "inherit",
  },
);
if (signingIdentity) {
  execFileSync(
    "codesign",
    ["--force", "--timestamp", "--sign", signingIdentity, output],
    { stdio: "inherit" },
  );
  execFileSync("codesign", ["--verify", "--verbose=2", output], {
    stdio: "inherit",
  });
}
execFileSync("hdiutil", ["verify", output], { stdio: "inherit" });
