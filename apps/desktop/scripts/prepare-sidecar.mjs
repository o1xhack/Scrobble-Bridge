import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

const appRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(appRoot, "../..");
const requestedTarget = process.env.SCROBBLE_BUILD_TARGET?.trim();
const cargoArguments = [
  "build",
  "--locked",
  "--release",
  "--package",
  "scrobble-native-host",
];
if (requestedTarget) cargoArguments.push("--target", requestedTarget);
execFileSync("cargo", cargoArguments, {
  cwd: repositoryRoot,
  stdio: "inherit",
});
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host: "))
  ?.slice(6);
if (!host) throw new Error("Could not determine the Rust host target");
const target = requestedTarget || host;
const extension = target.includes("windows") ? ".exe" : "";
const binaryDirectory = resolve(appRoot, "src-tauri/binaries");
mkdirSync(binaryDirectory, { recursive: true });
copyFileSync(
  resolve(
    repositoryRoot,
    requestedTarget ? `target/${target}/release` : "target/release",
    `scrobble-native-host${extension}`,
  ),
  resolve(binaryDirectory, `scrobble-native-host-${target}${extension}`),
);
