import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync, symlinkSync } from "node:fs";
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
execFileSync("pnpm", tauriArguments, {
  cwd: appRoot,
  env: createSigningEnvironment(process.env, signingIdentity),
  stdio: "inherit",
});
const bundleRoot = resolve(
  repositoryRoot,
  requestedTarget
    ? `target/${requestedTarget}/release/bundle`
    : "target/release/bundle",
);
const appPath = resolve(bundleRoot, "macos/Scrobble Bridge.app");
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
const output = resolve(
  bundleRoot,
  `dmg/Scrobble Bridge_1.0.0_${architecture}.dmg`,
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
